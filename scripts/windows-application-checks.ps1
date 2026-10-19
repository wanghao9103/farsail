# Windows-only checks shared by packaging and the disposable installation test.
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public static class FarSailApplicationChecks {
    private delegate bool WindowVisitor(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")]
    private static extern bool EnumWindows(WindowVisitor visitor, IntPtr parameter);
    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll")]
    private static extern IntPtr GetWindow(IntPtr window, uint command);
    [DllImport("user32.dll")]
    private static extern IntPtr GetWindowLongPtrW(IntPtr window, int index);
    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr FindResourceW(IntPtr module, IntPtr name, IntPtr type);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint SizeofResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll")]
    private static extern IntPtr LockResource(IntPtr resource);
    [DllImport("kernel32.dll")]
    private static extern bool FreeLibrary(IntPtr module);
    [DllImport("kernel32.dll")]
    private static extern bool CloseHandle(IntPtr handle);
    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool GetTokenInformation(IntPtr token, int kind, out int value, int size, out int returned);

    public static IntPtr FindMainWindow(int processId) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner != (uint)processId || !IsWindowVisible(window)) return true;
            // Exclude the single-instance plugin's ownerless notification tool window.
            if (GetWindow(window, 4) != IntPtr.Zero || (GetWindowLongPtrW(window, -20).ToInt64() & 0x80) != 0) return true;
            found = window;
            return false;
        }, IntPtr.Zero);
        return found;
    }

    public static string ReadManifest(string path) {
        // LOAD_LIBRARY_AS_DATAFILE: inspect resources without executing the application.
        IntPtr module = LoadLibraryExW(path, IntPtr.Zero, 2);
        if (module == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        try {
            // RT_MANIFEST (24), CREATEPROCESS_MANIFEST_RESOURCE_ID (1).
            IntPtr resource = FindResourceW(module, new IntPtr(1), new IntPtr(24));
            if (resource == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            uint size = SizeofResource(module, resource);
            if (size == 0 || size > 1024 * 1024) throw new InvalidOperationException("Invalid application manifest size");
            IntPtr data = LockResource(LoadResource(module, resource));
            if (data == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            byte[] bytes = new byte[checked((int)size)];
            Marshal.Copy(data, bytes, 0, bytes.Length);
            return Encoding.UTF8.GetString(bytes).TrimEnd('\0').TrimStart('\uFEFF');
        } finally { FreeLibrary(module); }
    }

    public static bool IsElevated(IntPtr process) {
        IntPtr token;
        if (!OpenProcessToken(process, 8, out token)) throw new Win32Exception(Marshal.GetLastWin32Error());
        try {
            int elevated, returned;
            if (!GetTokenInformation(token, 20, out elevated, sizeof(int), out returned))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            return elevated != 0;
        } finally { CloseHandle(token); }
    }
}
'@

function Assert-ApplicationManifest([string]$Executable) {
    [xml]$manifest = [FarSailApplicationChecks]::ReadManifest($Executable)
    $namespaces = [Xml.XmlNamespaceManager]::new($manifest.NameTable)
    $namespaces.AddNamespace('asm', 'urn:schemas-microsoft-com:asm.v1')
    $namespaces.AddNamespace('trust', 'urn:schemas-microsoft-com:asm.v3')
    $level = $manifest.SelectSingleNode('/asm:assembly/trust:trustInfo/trust:security/trust:requestedPrivileges/trust:requestedExecutionLevel', $namespaces)
    if (!$level -or $level.GetAttribute('level') -ne 'requireAdministrator' -or $level.GetAttribute('uiAccess') -ne 'false') {
        throw 'Application must request administrator privileges through normal Windows UAC'
    }
    $controls = $manifest.SelectSingleNode('/asm:assembly/asm:dependency/asm:dependentAssembly/asm:assemblyIdentity[@name="Microsoft.Windows.Common-Controls"]', $namespaces)
    if (!$controls -or $controls.GetAttribute('version') -ne '6.0.0.0') {
        throw 'Application manifest lost the native dialog Common Controls dependency'
    }
}

function Assert-ProcessElevated([Diagnostics.Process]$Process) {
    if (![FarSailApplicationChecks]::IsElevated($Process.Handle)) {
        throw "Expected elevated application process: $($Process.Id)"
    }
}

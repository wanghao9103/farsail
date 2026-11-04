package app.farsail.android

/** Saved choice, system authorization and live capability are separate facts. */
data class ControlStatus(
    val remembered: Boolean,
    val systemEnabled: Boolean,
    val connected: Boolean,
    val runtimeEnabled: Boolean,
    val sharing: Boolean,
    val automaticApproval: Boolean,
) {
    val active get() = remembered && systemEnabled && connected && runtimeEnabled
    val directConnection get() = active && sharing && automaticApproval
    val detail: String get() = when {
        !remembered -> "远程控制已关闭。开启后，同账号设备可免确认连接。"
        !systemEnabled -> "控制未生效：系统辅助功能已关闭。选择已记住，开启上方开关可重新授权。"
        !connected -> "控制未生效：系统授权已开启，等待 FarSail 辅助功能服务连接。"
        !runtimeEnabled -> "控制待恢复。选择已记住，可重新开启上方开关。"
        !sharing -> "控制权限已就绪；恢复屏幕共享后，同账号设备可免确认连接。"
        !automaticApproval -> "控制权限已就绪，正在恢复同账号免确认连接…"
        else -> "远程控制已生效，同账号设备连接无需再次批准。"
    }
}

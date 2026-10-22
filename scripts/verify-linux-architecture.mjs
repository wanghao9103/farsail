import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export const linuxArchitectures = {
  "x86_64-unknown-linux-gnu": { deb: "amd64", machine: 62, name: "x86_64" },
  "aarch64-unknown-linux-gnu": { deb: "arm64", machine: 183, name: "aarch64" },
};

export function verifyLinuxArchitecture(bytes, target) {
  const expected = linuxArchitectures[target];
  if (!expected) throw new Error("Unsupported Linux client target");
  if (
    bytes.length < 64 ||
    !bytes.subarray(0, 4).equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46])) ||
    bytes[4] !== 2 ||
    bytes[5] !== 1 ||
    bytes[6] !== 1 ||
    ![2, 3].includes(bytes.readUInt16LE(16)) ||
    bytes.readUInt16LE(18) !== expected.machine
  )
    throw new Error(`Expected 64-bit Linux ${expected.name} ELF for ${target}`);
  return expected;
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  const [, , binary, target] = process.argv;
  if (!binary || !target)
    throw new Error("Usage: verify-linux-architecture.mjs BINARY TARGET");
  const verified = verifyLinuxArchitecture(readFileSync(binary), target);
  console.log(
    `Verified ELF ${verified.name} (${verified.machine}) for ${target}`,
  );
}

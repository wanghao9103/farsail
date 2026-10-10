-- Keep historical migration checksums intact for existing installations.
ALTER TABLE devices DROP CONSTRAINT devices_platform_check;
ALTER TABLE devices ADD CONSTRAINT devices_platform_check
    CHECK (platform IN ('windows', 'linux', 'android', 'ios'));

export type Device = { id: string; name: string; platform: string; can_host: boolean; can_files: boolean; online: boolean; enabled: boolean; last_seen_at: string | null };
export type Me = { id: string; email: string; role: 'user' | 'admin'; session_id: string };
export type Remote = { id: string; requester_id: string; source_device_id: string; target_device_id: string; permission: 'view' | 'control' | 'files'; state: string; grant_expires_at: string | null };
export type Pending = { id: string; requester_id: string; source_device_id: string; permission: string };
export type User = { id: string; email: string; enabled: boolean; verified: boolean; role: string };

CREATE TABLE users (
 id uuid PRIMARY KEY, email text NOT NULL UNIQUE, password_hash text NOT NULL,
 verified boolean NOT NULL DEFAULT false, enabled boolean NOT NULL DEFAULT true,
 role text NOT NULL DEFAULT 'user' CHECK (role IN ('user','admin')),
 credential_version bigint NOT NULL DEFAULT 0,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE email_tokens (
 id uuid PRIMARY KEY, user_id uuid NOT NULL REFERENCES users(id), kind text NOT NULL CHECK (kind IN ('verify','reset')),
 token_hash bytea NOT NULL UNIQUE, expires_at timestamptz NOT NULL, consumed_at timestamptz
);
CREATE TABLE auth_sessions (
 id uuid PRIMARY KEY, user_id uuid NOT NULL REFERENCES users(id), access_hash bytea NOT NULL UNIQUE,
 refresh_hash bytea NOT NULL UNIQUE, access_expires_at timestamptz NOT NULL,
 refresh_expires_at timestamptz NOT NULL, revoked_at timestamptz,
 created_at timestamptz NOT NULL DEFAULT now(), last_used_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE used_refresh_tokens (
 token_hash bytea PRIMARY KEY, session_id uuid NOT NULL REFERENCES auth_sessions(id), used_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE devices (
 id uuid PRIMARY KEY, owner_id uuid NOT NULL REFERENCES users(id), public_key bytea NOT NULL UNIQUE,
 name text NOT NULL, platform text NOT NULL CHECK (platform IN ('windows','android','ios')),
 can_host boolean NOT NULL, can_files boolean NOT NULL, enabled boolean NOT NULL DEFAULT true,
 bound boolean NOT NULL DEFAULT true,
 credential_hash bytea NOT NULL UNIQUE, credential_session_id uuid NOT NULL REFERENCES auth_sessions(id),
 generation bigint NOT NULL DEFAULT 0,
 lease_until timestamptz, last_seen_at timestamptz, created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX devices_owner ON devices(owner_id,id);
CREATE TABLE device_challenges (
 id uuid PRIMARY KEY, owner_id uuid NOT NULL REFERENCES users(id), public_key bytea NOT NULL,
 nonce bytea NOT NULL, expires_at timestamptz NOT NULL, consumed_at timestamptz
);
CREATE TABLE invitations (
 id uuid PRIMARY KEY, creator_id uuid NOT NULL REFERENCES users(id), target_device_id uuid NOT NULL REFERENCES devices(id),
 code_hash bytea NOT NULL UNIQUE, permission text NOT NULL CHECK (permission IN ('view','control','files')),
 expires_at timestamptz NOT NULL, consumed_at timestamptz, revoked_at timestamptz
);
CREATE TABLE signup_invitations (
 id uuid PRIMARY KEY, created_by uuid NOT NULL REFERENCES users(id), email text NOT NULL,
 code_hash bytea NOT NULL UNIQUE, expires_at timestamptz NOT NULL,
 consumed_at timestamptz, revoked_at timestamptz
);
CREATE TABLE remote_sessions (
 id uuid PRIMARY KEY, requester_id uuid NOT NULL REFERENCES users(id), source_device_id uuid NOT NULL REFERENCES devices(id),
 target_device_id uuid NOT NULL REFERENCES devices(id), permission text NOT NULL CHECK (permission IN ('view','control','files')),
 state text NOT NULL CHECK (state IN ('pending','approved','denied','revoked')),
 nonce bytea NOT NULL, grant_hash bytea UNIQUE, grant_until timestamptz,
 auth_session_id uuid NOT NULL REFERENCES auth_sessions(id), invitation_id uuid REFERENCES invitations(id),
 created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX remote_sessions_target ON remote_sessions(target_device_id,state);
CREATE TABLE settings (key text PRIMARY KEY, value text NOT NULL);
INSERT INTO settings(key,value) VALUES ('registration','open');
CREATE TABLE audit_events (
 id bigserial PRIMARY KEY, actor_id uuid, action text NOT NULL, object_id uuid, result text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE rate_limits (
 key text PRIMARY KEY, attempts integer NOT NULL, window_start timestamptz NOT NULL
);

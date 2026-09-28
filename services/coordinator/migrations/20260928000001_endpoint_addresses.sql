CREATE TABLE device_endpoint_addresses (
 device_id uuid PRIMARY KEY REFERENCES devices(id),
 generation bigint NOT NULL,
 endpoint_addr text NOT NULL,
 updated_at timestamptz NOT NULL DEFAULT now()
);

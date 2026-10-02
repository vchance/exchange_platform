-- Runs once when the development database container is first created.
-- The container's own user (`exchange`) is the schema owner; this adds the
-- restricted role the api and worker connect as.
CREATE ROLE exchange_app LOGIN PASSWORD 'exchange_app';

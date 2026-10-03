-- What the migrations give the application role and which triggers they
-- create, one line each, sorted: the inventory scripts/restore.sh checks a
-- restored database against (scripts/restore-inventory.txt), and
-- backend/tests/schema.rs and backend/tests/inventory.rs check the
-- migrations against. Run with psql, with the role's name in :'app_role'.
--
--   grant relation TABLE PRIVILEGE     a privilege on a table, view or sequence
--   grant column TABLE.COLUMN PRIVILEGE
--   grant function NAME PRIVILEGE
--   grant schema public PRIVILEGE
--   trigger TABLE NAME ENABLED         every trigger that is not internal,
--                                      with pg_trigger.tgenabled (O: enabled)
SELECT line FROM (
    SELECT 'grant relation ' || c.relname || ' ' || a.privilege_type AS line
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = 'public'
    CROSS JOIN LATERAL aclexplode(c.relacl) a
    JOIN pg_roles r ON r.oid = a.grantee AND r.rolname = :'app_role'
    UNION ALL
    SELECT 'grant column ' || c.relname || '.' || att.attname || ' ' || a.privilege_type
    FROM pg_attribute att
    JOIN pg_class c ON c.oid = att.attrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = 'public'
    CROSS JOIN LATERAL aclexplode(att.attacl) a
    JOIN pg_roles r ON r.oid = a.grantee AND r.rolname = :'app_role'
    UNION ALL
    SELECT 'grant function ' || p.proname || ' ' || a.privilege_type
    FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace AND n.nspname = 'public'
    CROSS JOIN LATERAL aclexplode(p.proacl) a
    JOIN pg_roles r ON r.oid = a.grantee AND r.rolname = :'app_role'
    UNION ALL
    SELECT 'grant schema public ' || privilege
    FROM (VALUES ('USAGE'), ('CREATE')) AS s(privilege)
    WHERE has_schema_privilege(:'app_role', 'public', privilege)
    UNION ALL
    SELECT 'trigger ' || c.relname || ' ' || t.tgname || ' ' || t.tgenabled::text
    FROM pg_trigger t
    JOIN pg_class c ON c.oid = t.tgrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = 'public'
    WHERE NOT t.tgisinternal
) AS inventory
ORDER BY line COLLATE "C";

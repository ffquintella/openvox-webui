-- Add os_version_pattern to repository_version_catalog to separate
-- packages by OS major version (e.g., el8 vs el9)

ALTER TABLE repository_version_catalog ADD COLUMN os_version_pattern TEXT;

-- Recreate the unique index to include os_version_pattern and source_kind
DROP INDEX IF EXISTS idx_repository_version_catalog_identity;

CREATE UNIQUE INDEX idx_repository_version_catalog_identity
    ON repository_version_catalog(
        platform_family,
        distribution,
        package_manager,
        software_type,
        software_name,
        COALESCE(repository_source, ''),
        source_kind,
        COALESCE(os_version_pattern, '')
    );

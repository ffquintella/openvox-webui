//! Backup service
//!
//! Main orchestration service for server backup and restore operations.
//! Handles backup creation, encryption, restoration, and cleanup.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tar::{Archive, Builder};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::BackupConfig;
use crate::db::BackupRepository;
use crate::models::{
    BackupRestore, BackupSchedule, BackupStatus, BackupTrigger, ServerBackup,
    VerifyBackupResponse,
};
use crate::services::backup_encryption::{self, EncryptedData};

/// Backup service for managing server backups
pub struct BackupService {
    pool: SqlitePool,
    config: BackupConfig,
    /// Lock to ensure only one backup/restore runs at a time
    operation_lock: Arc<Mutex<()>>,
}

impl BackupService {
    /// Create a new backup service
    pub fn new(pool: SqlitePool, config: BackupConfig) -> Self {
        Self {
            pool,
            config,
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Check if backup feature is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the backup configuration
    pub fn config(&self) -> &BackupConfig {
        &self.config
    }

    /// Get the backup directory path
    pub fn backup_dir(&self) -> &Path {
        &self.config.backup_dir
    }

    /// Ensure backup directory exists and is writable
    pub fn ensure_backup_dir(&self) -> Result<()> {
        let dir = &self.config.backup_dir;

        if !dir.exists() {
            fs::create_dir_all(dir)
                .with_context(|| format!("Failed to create backup directory: {:?}", dir))?;
            info!("Created backup directory: {:?}", dir);
        }

        // Test write permissions
        let test_file = dir.join(".write_test");
        fs::write(&test_file, b"test")
            .with_context(|| format!("Backup directory is not writable: {:?}", dir))?;
        fs::remove_file(&test_file).ok();

        Ok(())
    }

    /// Check if backup directory exists and is writable
    pub fn check_backup_dir(&self) -> (bool, bool) {
        let dir = &self.config.backup_dir;
        let exists = dir.exists();

        let writable = if exists {
            let test_file = dir.join(".write_test");
            let result = fs::write(&test_file, b"test").is_ok();
            fs::remove_file(&test_file).ok();
            result
        } else {
            false
        };

        (exists, writable)
    }

    // =========================================================================
    // Backup Operations
    // =========================================================================

    /// Create a new backup
    pub async fn create_backup(
        &self,
        password: Option<&str>,
        notes: Option<&str>,
        trigger: BackupTrigger,
        created_by: Option<Uuid>,
        include_database: bool,
        include_config: bool,
    ) -> Result<ServerBackup> {
        // Acquire lock to prevent concurrent backup/restore
        let _lock = self.operation_lock.lock().await;

        // Ensure backup directory exists
        self.ensure_backup_dir()?;

        let backup_id = Uuid::new_v4();
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let filename = format!("backup_{}_{}.tar.gz", timestamp, &backup_id.to_string()[..8]);
        let file_path = self.config.backup_dir.join(&filename);

        // Create initial backup record
        let mut backup = ServerBackup {
            id: backup_id,
            filename: filename.clone(),
            file_path: file_path.to_string_lossy().to_string(),
            file_size: 0,
            checksum: String::new(),
            uncompressed_size: None,
            is_encrypted: password.is_some() && self.config.encryption.enabled,
            encryption_salt: None,
            encryption_nonce: None,
            trigger_type: trigger,
            status: BackupStatus::InProgress,
            error_message: None,
            started_at: Some(Utc::now()),
            completed_at: None,
            created_by,
            includes_database: include_database,
            includes_config: include_config,
            database_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            notes: notes.map(String::from),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let repo = BackupRepository::new(self.pool.clone());
        repo.create_backup(&backup).await?;

        // Collect files to backup
        let files_to_backup = self.collect_backup_files(include_database, include_config)?;

        if files_to_backup.is_empty() {
            repo.fail_backup(backup_id, "No files to backup").await?;
            return Err(anyhow::anyhow!("No files to backup"));
        }

        info!("Creating backup with {} files", files_to_backup.len());

        // Create tar.gz archive
        match self.create_archive(&files_to_backup) {
            Ok((archive_data, uncompressed_size)) => {
                backup.uncompressed_size = Some(uncompressed_size as i64);

                // Encrypt if password provided
                let (final_data, encrypted) = if let Some(pwd) = password {
                    if self.config.encryption.enabled {
                        match backup_encryption::encrypt(&archive_data, pwd) {
                            Ok(encrypted_data) => {
                                backup.encryption_salt = Some(encrypted_data.salt_base64());
                                backup.encryption_nonce = Some(encrypted_data.nonce_base64());
                                (encrypted_data.ciphertext.clone(), true)
                            }
                            Err(e) => {
                                let msg = format!("Encryption failed: {}", e);
                                error!("{}", msg);
                                repo.fail_backup(backup_id, &msg).await?;
                                return Err(e);
                            }
                        }
                    } else {
                        (archive_data, false)
                    }
                } else {
                    (archive_data, false)
                };

                backup.is_encrypted = encrypted;

                // Write to file
                if let Err(e) = fs::write(&file_path, &final_data) {
                    let msg = format!("Failed to write backup file: {}", e);
                    error!("{}", msg);
                    repo.fail_backup(backup_id, &msg).await?;
                    return Err(e.into());
                }

                // Calculate checksum
                let checksum = calculate_sha256(&final_data);
                let file_size = final_data.len() as i64;

                // Update backup record with completion info
                repo.complete_backup(backup_id, file_size, &checksum, backup.uncompressed_size)
                    .await?;

                backup.file_size = file_size;
                backup.checksum = checksum;
                backup.status = BackupStatus::Completed;
                backup.completed_at = Some(Utc::now());

                info!(
                    "Backup created successfully: {} ({} bytes)",
                    filename, file_size
                );

                Ok(backup)
            }
            Err(e) => {
                let msg = format!("Failed to create archive: {}", e);
                error!("{}", msg);
                repo.fail_backup(backup_id, &msg).await?;
                Err(e)
            }
        }
    }

    /// Collect files to include in the backup
    fn collect_backup_files(
        &self,
        include_database: bool,
        include_config: bool,
    ) -> Result<Vec<(PathBuf, String)>> {
        let mut files = Vec::new();

        if include_database && self.config.include.database {
            // Database files - extract path from sqlite URL
            if let Some(db_path) = self.get_database_path() {
                // Main database file
                if db_path.exists() {
                    files.push((db_path.clone(), "database/openvox.db".to_string()));
                }
                // WAL file
                let wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
                if wal_path.exists() {
                    files.push((wal_path, "database/openvox.db-wal".to_string()));
                }
                // SHM file
                let shm_path = PathBuf::from(format!("{}-shm", db_path.display()));
                if shm_path.exists() {
                    files.push((shm_path, "database/openvox.db-shm".to_string()));
                }
            }
        }

        if include_config && self.config.include.config_files {
            // Configuration files
            let config_paths = [
                "/etc/openvox-webui/config.yaml",
                "/etc/openvox-webui/groups.yaml",
            ];

            for config_path in config_paths {
                let path = PathBuf::from(config_path);
                if path.exists() {
                    let name = path.file_name().unwrap().to_string_lossy().to_string();
                    files.push((path, format!("config/{}", name)));
                }
            }
        }

        Ok(files)
    }

    /// Get the database path from environment or default
    fn get_database_path(&self) -> Option<PathBuf> {
        // Try environment variable first
        if let Ok(url) = std::env::var("DATABASE_URL") {
            return extract_path_from_sqlite_url(&url);
        }

        // Default path
        let default_path = PathBuf::from("/var/lib/openvox-webui/openvox.db");
        if default_path.exists() {
            return Some(default_path);
        }

        // Development fallback
        let dev_path = PathBuf::from("openvox.db");
        if dev_path.exists() {
            return Some(dev_path);
        }

        None
    }

    /// Create a tar.gz archive from the given files
    fn create_archive(&self, files: &[(PathBuf, String)]) -> Result<(Vec<u8>, usize)> {
        let mut archive_data = Vec::new();
        let encoder = GzEncoder::new(&mut archive_data, Compression::default());
        let mut builder = Builder::new(encoder);
        let mut total_size = 0usize;

        for (src_path, archive_name) in files {
            debug!("Adding to archive: {:?} -> {}", src_path, archive_name);

            let mut file = File::open(src_path)
                .with_context(|| format!("Failed to open file: {:?}", src_path))?;

            let metadata = file.metadata()?;
            total_size += metadata.len() as usize;

            let mut header = tar::Header::new_gnu();
            header.set_path(archive_name)?;
            header.set_size(metadata.len());
            header.set_mode(0o644);
            header.set_mtime(
                metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            );
            header.set_cksum();

            builder.append(&header, &mut file)?;
        }

        builder.finish()?;
        drop(builder);

        Ok((archive_data, total_size))
    }

    // =========================================================================
    // Restore Operations
    // =========================================================================

    /// Restore from a backup
    pub async fn restore_backup(
        &self,
        backup_id: Uuid,
        password: &str,
        restored_by: Option<Uuid>,
    ) -> Result<BackupRestore> {
        // Acquire lock to prevent concurrent backup/restore
        let _lock = self.operation_lock.lock().await;

        let repo = BackupRepository::new(self.pool.clone());

        // Get backup record
        let backup = repo
            .get_backup(backup_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Backup not found"))?;

        if backup.status != BackupStatus::Completed {
            return Err(anyhow::anyhow!("Cannot restore from incomplete backup"));
        }

        // Create restore record
        let restore = BackupRestore {
            id: Uuid::new_v4(),
            backup_id,
            status: BackupStatus::InProgress,
            error_message: None,
            started_at: Some(Utc::now()),
            completed_at: None,
            restored_by,
            created_at: Utc::now(),
        };
        repo.create_restore(&restore).await?;

        // Read backup file
        let backup_data = fs::read(&backup.file_path)
            .with_context(|| format!("Failed to read backup file: {}", backup.file_path))?;

        // Verify checksum
        let checksum = calculate_sha256(&backup_data);
        if checksum != backup.checksum {
            let msg = "Checksum mismatch - backup file may be corrupted";
            repo.update_restore_status(restore.id, BackupStatus::Failed, Some(msg))
                .await?;
            return Err(anyhow::anyhow!(msg));
        }

        // Decrypt if encrypted
        let archive_data = if backup.is_encrypted {
            let salt = backup
                .encryption_salt
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing encryption salt"))?;
            let nonce = backup
                .encryption_nonce
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing encryption nonce"))?;

            let encrypted = EncryptedData::from_base64(salt, nonce, backup_data)?;

            match backup_encryption::decrypt(&encrypted, password) {
                Ok(data) => data,
                Err(e) => {
                    let msg = format!("Decryption failed: {}", e);
                    repo.update_restore_status(restore.id, BackupStatus::Failed, Some(&msg))
                        .await?;
                    return Err(anyhow::anyhow!(msg));
                }
            }
        } else {
            backup_data
        };

        // Extract archive
        match self.extract_archive(&archive_data, backup.includes_database, backup.includes_config) {
            Ok(_) => {
                repo.update_restore_status(restore.id, BackupStatus::Completed, None)
                    .await?;
                info!("Restore completed successfully from backup: {}", backup.filename);

                // Return updated restore record
                repo.get_restore(restore.id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("Failed to fetch restore record"))
            }
            Err(e) => {
                let msg = format!("Failed to extract archive: {}", e);
                error!("{}", msg);
                repo.update_restore_status(restore.id, BackupStatus::Failed, Some(&msg))
                    .await?;
                Err(anyhow::anyhow!(msg))
            }
        }
    }

    /// Extract tar.gz archive to restore files
    fn extract_archive(
        &self,
        archive_data: &[u8],
        includes_database: bool,
        includes_config: bool,
    ) -> Result<()> {
        let decoder = GzDecoder::new(archive_data);
        let mut archive = Archive::new(decoder);

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            let path_str = path.to_string_lossy();

            // Determine destination based on archive path
            let dest = if path_str.starts_with("database/") && includes_database {
                // Extract database files
                if let Some(db_path) = self.get_database_path() {
                    let db_dir = db_path.parent().unwrap_or(Path::new("."));
                    let filename = path.file_name().unwrap();
                    Some(db_dir.join(filename))
                } else {
                    warn!("Cannot determine database path for restore");
                    None
                }
            } else if path_str.starts_with("config/") && includes_config {
                // Extract config files to /etc/openvox-webui/
                let filename = path.file_name().unwrap();
                Some(PathBuf::from("/etc/openvox-webui").join(filename))
            } else {
                None
            };

            if let Some(dest_path) = dest {
                debug!("Extracting: {} -> {:?}", path_str, dest_path);

                // Ensure parent directory exists
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                // Create backup of existing file
                if dest_path.exists() {
                    let backup_name = format!("{}.backup", dest_path.display());
                    fs::rename(&dest_path, &backup_name).ok();
                }

                // Extract file
                let mut contents = Vec::new();
                entry.read_to_end(&mut contents)?;
                fs::write(&dest_path, &contents)?;
            }
        }

        Ok(())
    }

    // =========================================================================
    // Verification
    // =========================================================================

    /// Verify backup integrity and decryption
    pub async fn verify_backup(
        &self,
        backup_id: Uuid,
        password: &str,
    ) -> Result<VerifyBackupResponse> {
        let repo = BackupRepository::new(self.pool.clone());

        let backup = repo
            .get_backup(backup_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Backup not found"))?;

        // Read backup file
        let backup_data = match fs::read(&backup.file_path) {
            Ok(data) => data,
            Err(e) => {
                return Ok(VerifyBackupResponse {
                    valid: false,
                    checksum_match: false,
                    can_decrypt: false,
                    file_count: None,
                    total_size: None,
                    error: Some(format!("Failed to read backup file: {}", e)),
                });
            }
        };

        // Verify checksum
        let checksum = calculate_sha256(&backup_data);
        let checksum_match = checksum == backup.checksum;

        if !checksum_match {
            return Ok(VerifyBackupResponse {
                valid: false,
                checksum_match: false,
                can_decrypt: false,
                file_count: None,
                total_size: None,
                error: Some("Checksum mismatch - backup file may be corrupted".to_string()),
            });
        }

        // Try to decrypt if encrypted
        let archive_data = if backup.is_encrypted {
            let salt = match backup.encryption_salt.as_ref() {
                Some(s) => s,
                None => {
                    return Ok(VerifyBackupResponse {
                        valid: false,
                        checksum_match: true,
                        can_decrypt: false,
                        file_count: None,
                        total_size: None,
                        error: Some("Missing encryption salt".to_string()),
                    });
                }
            };

            let nonce = match backup.encryption_nonce.as_ref() {
                Some(n) => n,
                None => {
                    return Ok(VerifyBackupResponse {
                        valid: false,
                        checksum_match: true,
                        can_decrypt: false,
                        file_count: None,
                        total_size: None,
                        error: Some("Missing encryption nonce".to_string()),
                    });
                }
            };

            match EncryptedData::from_base64(salt, nonce, backup_data) {
                Ok(encrypted) => match backup_encryption::decrypt(&encrypted, password) {
                    Ok(data) => data,
                    Err(_) => {
                        return Ok(VerifyBackupResponse {
                            valid: false,
                            checksum_match: true,
                            can_decrypt: false,
                            file_count: None,
                            total_size: None,
                            error: Some("Decryption failed - incorrect password".to_string()),
                        });
                    }
                },
                Err(e) => {
                    return Ok(VerifyBackupResponse {
                        valid: false,
                        checksum_match: true,
                        can_decrypt: false,
                        file_count: None,
                        total_size: None,
                        error: Some(format!("Invalid encryption data: {}", e)),
                    });
                }
            }
        } else {
            backup_data
        };

        // Try to read archive contents
        let decoder = GzDecoder::new(&archive_data[..]);
        let mut archive = Archive::new(decoder);

        let mut file_count = 0usize;
        let mut total_size = 0i64;

        match archive.entries() {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(e) => {
                            file_count += 1;
                            total_size += e.size() as i64;
                        }
                        Err(e) => {
                            return Ok(VerifyBackupResponse {
                                valid: false,
                                checksum_match: true,
                                can_decrypt: backup.is_encrypted,
                                file_count: None,
                                total_size: None,
                                error: Some(format!("Archive corrupted: {}", e)),
                            });
                        }
                    }
                }
            }
            Err(e) => {
                return Ok(VerifyBackupResponse {
                    valid: false,
                    checksum_match: true,
                    can_decrypt: backup.is_encrypted,
                    file_count: None,
                    total_size: None,
                    error: Some(format!("Failed to read archive: {}", e)),
                });
            }
        }

        Ok(VerifyBackupResponse {
            valid: true,
            checksum_match: true,
            can_decrypt: true,
            file_count: Some(file_count),
            total_size: Some(total_size),
            error: None,
        })
    }

    // =========================================================================
    // Cleanup & Management
    // =========================================================================

    /// Delete a backup
    pub async fn delete_backup(&self, backup_id: Uuid) -> Result<bool> {
        let repo = BackupRepository::new(self.pool.clone());

        let backup = match repo.get_backup(backup_id).await? {
            Some(b) => b,
            None => return Ok(false),
        };

        // Delete the file if it exists
        let file_path = PathBuf::from(&backup.file_path);
        if file_path.exists() {
            fs::remove_file(&file_path)
                .with_context(|| format!("Failed to delete backup file: {:?}", file_path))?;
            info!("Deleted backup file: {:?}", file_path);
        }

        // Delete the database record
        repo.delete_backup(backup_id).await?;

        Ok(true)
    }

    /// Cleanup old backups based on retention policy
    pub async fn cleanup_old_backups(&self) -> Result<usize> {
        let repo = BackupRepository::new(self.pool.clone());

        // Get schedule to check retention count
        let retention_count = match repo.get_schedule().await? {
            Some(schedule) => schedule.retention_count as u32,
            None => self.config.retention.max_backups,
        };

        let old_backups = repo.get_backups_exceeding_retention(retention_count).await?;
        let mut deleted_count = 0;

        for backup in old_backups {
            // Check minimum age
            let age_hours = (Utc::now() - backup.created_at).num_hours();
            if age_hours < self.config.retention.min_age_hours as i64 {
                debug!(
                    "Skipping backup {} - not old enough ({} hours < {} hours)",
                    backup.filename, age_hours, self.config.retention.min_age_hours
                );
                continue;
            }

            if let Err(e) = self.delete_backup(backup.id).await {
                warn!("Failed to delete old backup {}: {}", backup.filename, e);
            } else {
                deleted_count += 1;
                info!("Deleted old backup: {}", backup.filename);
            }
        }

        Ok(deleted_count)
    }

    /// Get backup download path
    pub fn get_backup_path(&self, backup: &ServerBackup) -> Option<PathBuf> {
        let path = PathBuf::from(&backup.file_path);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    // =========================================================================
    // Repository Helpers
    // =========================================================================

    /// List backups
    pub async fn list_backups(
        &self,
        status: Option<&str>,
        trigger_type: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ServerBackup>> {
        let repo = BackupRepository::new(self.pool.clone());
        repo.list_backups(status, trigger_type, limit, offset).await
    }

    /// Get a backup by ID
    pub async fn get_backup(&self, id: Uuid) -> Result<Option<ServerBackup>> {
        let repo = BackupRepository::new(self.pool.clone());
        repo.get_backup(id).await
    }

    /// Get backup statistics
    pub async fn get_stats(&self) -> Result<(i64, i64)> {
        let repo = BackupRepository::new(self.pool.clone());
        repo.get_backup_stats().await
    }

    /// Get last completed backup
    pub async fn get_last_backup(&self) -> Result<Option<ServerBackup>> {
        let repo = BackupRepository::new(self.pool.clone());
        repo.get_last_backup().await
    }

    /// Get backup schedule
    pub async fn get_schedule(&self) -> Result<Option<BackupSchedule>> {
        let repo = BackupRepository::new(self.pool.clone());
        repo.get_schedule().await
    }

    /// Update backup schedule
    pub async fn update_schedule(&self, schedule: &BackupSchedule) -> Result<()> {
        let repo = BackupRepository::new(self.pool.clone());
        repo.update_schedule(schedule).await
    }

    /// List restore history
    pub async fn list_restores(&self, limit: u32) -> Result<Vec<BackupRestore>> {
        let repo = BackupRepository::new(self.pool.clone());
        repo.list_restores(limit).await
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Calculate SHA-256 checksum of data
fn calculate_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Extract file path from SQLite URL
fn extract_path_from_sqlite_url(url: &str) -> Option<PathBuf> {
    // Handle formats like "sqlite:///path/to/db.sqlite" or "sqlite:path/to/db.sqlite"
    url.strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_sha256() {
        let data = b"Hello, World!";
        let hash = calculate_sha256(data);
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[test]
    fn test_extract_path_from_sqlite_url() {
        assert_eq!(
            extract_path_from_sqlite_url("sqlite:///var/lib/openvox/db.sqlite"),
            Some(PathBuf::from("/var/lib/openvox/db.sqlite"))
        );
        assert_eq!(
            extract_path_from_sqlite_url("sqlite:./data/db.sqlite"),
            Some(PathBuf::from("./data/db.sqlite"))
        );
        assert_eq!(extract_path_from_sqlite_url("postgres://localhost/db"), None);
    }
}

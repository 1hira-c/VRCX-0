use std::path::PathBuf;

use serde::Serialize;
use vrcx_0_application_core::Result;
use vrcx_0_contracts::{
    DatabaseUpgradeStatus, LegacyMigrationPaths, LegacyMigrationProgress, LegacyVrcxSource,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DatabaseUpgradePreflightStatus {
    Current,
    UpgradeRequired,
    Running,
    Finished,
    Blocked,
    NewerSchema,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseUpgradePreflight {
    pub status: DatabaseUpgradePreflightStatus,
    pub from_version: i64,
    pub to_version: i64,
    pub repair_pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<DatabaseUpgradeStage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<DatabaseUpgradeRunResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_upgrade: Option<DatabaseUpgradeStatus>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DatabaseUpgradeRunStatus {
    Current,
    Upgraded,
    Blocked,
    NewerSchema,
    Failed,
}

impl DatabaseUpgradeRunStatus {
    pub fn is_ready(self) -> bool {
        matches!(self, Self::Current | Self::Upgraded)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DatabaseUpgradeStage {
    Preflight,
    PrepareLegacySnapshot,
    PrepareLegacyConfiguration,
    FinalizeLegacyMigration,
    InitializeSchema,
    CreateWorkCopy,
    LegacySchemaMigration,
    LegacyPerformanceIndexes,
    GlobalPerformanceIndexes,
    NotificationPerformanceIndexes,
    SchemaMigrations,
    Optimize,
    WriteVersion,
    Commit,
    RepairData,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseUpgradeProgress {
    pub stage: DatabaseUpgradeStage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_units: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_units: Option<u64>,
}

impl DatabaseUpgradeProgress {
    pub fn indeterminate(stage: DatabaseUpgradeStage) -> Self {
        Self {
            stage,
            completed_units: None,
            total_units: None,
        }
    }

    pub fn determinate(
        stage: DatabaseUpgradeStage,
        completed_units: u64,
        total_units: u64,
    ) -> Self {
        Self {
            stage,
            completed_units: Some(completed_units),
            total_units: Some(total_units),
        }
    }
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseUpgradeRunResult {
    pub status: DatabaseUpgradeRunStatus,
    pub from_version: i64,
    pub to_version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_stage: Option<DatabaseUpgradeStage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_upgrade: Option<DatabaseUpgradeStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_warning: Option<String>,
}

pub trait DatabaseUpgradeStore: Send + Sync {
    fn schema_version(&self) -> i64;
    fn preflight(&self) -> Result<DatabaseUpgradePreflight>;
    fn run(&self, on_progress: &mut dyn FnMut(DatabaseUpgradeProgress))
        -> DatabaseUpgradeRunResult;
    fn discard_failed_upgrade(&self) -> Result<()>;
    fn archive_main_database_and_create_fresh_database(&self) -> Result<PathBuf>;
    fn prepare_legacy_migration(
        &self,
        paths: &LegacyMigrationPaths,
        source: &LegacyVrcxSource,
        on_progress: &mut dyn FnMut(LegacyMigrationProgress),
    ) -> Result<()>;
}

pub fn database_upgrade_preflight(
    store: &dyn DatabaseUpgradeStore,
) -> Result<DatabaseUpgradePreflight> {
    store.preflight()
}

pub fn run_database_upgrade(store: &dyn DatabaseUpgradeStore) -> DatabaseUpgradeRunResult {
    store.run(&mut |_| {})
}

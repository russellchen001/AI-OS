//! Where person profiles live between sessions.
//!
//! The store is **append-only**. Every revision of a profile is its own row and
//! nothing is ever updated in place, because the lifecycle above it already
//! depends on that: rollback republishes an earlier revision as a new one, and a
//! revision history that could be silently rewritten would make the ledger a
//! record of the present rather than of what happened.
//!
//! Each row also carries the evidence bundle that revision was drafted from.
//! Review re-verifies every claim against that bundle, so a profile whose bundle
//! was not kept would become unreviewable — the claims would have to be trusted
//! instead of checked.

use super::{
    evidence::EvidenceBundle,
    profile::{PersonDistillationProfile, ProfileStatus},
    review::ProfileLedger,
};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::{fs, path::PathBuf};

const DATABASE_DIRECTORY: &str = "AI-OS";
const DATABASE_FILE: &str = "person_profiles.sqlite3";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileSummary {
    pub profile_id: String,
    pub latest_revision: u32,
    pub active_revision: Option<u32>,
    pub status: ProfileStatus,
}

pub(crate) struct ProfileStore {
    path: PathBuf,
}

impl ProfileStore {
    pub(crate) fn open_default() -> Result<Self, String> {
        let directory = dirs::data_dir()
            .ok_or_else(|| "AI-OS data directory is unavailable".to_owned())?
            .join(DATABASE_DIRECTORY);
        fs::create_dir_all(&directory)
            .map_err(|_| "AI-OS could not create its profile storage".to_owned())?;
        Self::open_at(directory.join(DATABASE_FILE))
    }

    pub(crate) fn open_at(path: PathBuf) -> Result<Self, String> {
        let store = Self { path };
        store.connect()?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, String> {
        let connection = Connection::open(&self.path)
            .map_err(|_| "AI-OS could not open its profile database".to_owned())?;
        connection
            .execute_batch(
                "
                PRAGMA journal_mode = WAL;
                CREATE TABLE IF NOT EXISTS profile_revisions (
                    profile_id TEXT NOT NULL,
                    revision INTEGER NOT NULL,
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    status TEXT NOT NULL,
                    profile_json TEXT NOT NULL,
                    bundle_json TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS profile_revisions_by_profile
                    ON profile_revisions (profile_id, sequence);
                ",
            )
            .map_err(|_| "AI-OS could not prepare its profile database".to_owned())?;
        Ok(connection)
    }

    /// Append one revision. Never an update: see the module note.
    pub(crate) fn append(
        &self,
        profile: &PersonDistillationProfile,
        bundle: &EvidenceBundle,
    ) -> Result<(), String> {
        if profile.evidence_bundle_id != bundle.bundle_id {
            return Err(
                "A profile revision must be stored with the evidence bundle it was drafted from."
                    .to_owned(),
            );
        }
        let profile_json = serde_json::to_string(profile)
            .map_err(|_| "AI-OS could not serialize the profile revision.".to_owned())?;
        let bundle_json = serde_json::to_string(bundle)
            .map_err(|_| "AI-OS could not serialize the evidence bundle.".to_owned())?;
        self.connect()?
            .execute(
                "INSERT INTO profile_revisions
                    (profile_id, revision, status, profile_json, bundle_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
                params![
                    profile.profile_id,
                    profile.revision,
                    serde_json::to_string(&profile.status).unwrap_or_default(),
                    profile_json,
                    bundle_json,
                ],
            )
            .map_err(|_| "AI-OS could not store the profile revision.".to_owned())?;
        Ok(())
    }

    /// Every stored revision of one profile, oldest first, with its bundle.
    pub(crate) fn history(
        &self,
        profile_id: &str,
    ) -> Result<Vec<(PersonDistillationProfile, EvidenceBundle)>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT profile_json, bundle_json FROM profile_revisions
                 WHERE profile_id = ?1 ORDER BY sequence ASC",
            )
            .map_err(|_| "AI-OS could not read its profile database.".to_owned())?;
        let rows = statement
            .query_map(params![profile_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| "AI-OS could not read the stored profile revisions.".to_owned())?;

        let mut history = Vec::new();
        for row in rows {
            let (profile_json, bundle_json) =
                row.map_err(|_| "AI-OS could not read a stored profile revision.".to_owned())?;
            let profile = serde_json::from_str(&profile_json)
                .map_err(|_| "A stored profile revision is unreadable.".to_owned())?;
            let bundle = serde_json::from_str(&bundle_json)
                .map_err(|_| "A stored evidence bundle is unreadable.".to_owned())?;
            history.push((profile, bundle));
        }
        Ok(history)
    }

    pub(crate) fn ledger(&self, profile_id: &str) -> Result<ProfileLedger, String> {
        let mut ledger = ProfileLedger::default();
        for (profile, _) in self.history(profile_id)? {
            ledger
                .record(profile)
                .map_err(|error| format!("Stored profile history is inconsistent: {error}"))?;
        }
        Ok(ledger)
    }

    /// The newest stored revision and the bundle it was drafted from.
    pub(crate) fn latest(
        &self,
        profile_id: &str,
    ) -> Result<(PersonDistillationProfile, EvidenceBundle), String> {
        self.history(profile_id)?
            .pop()
            .ok_or_else(|| format!("No stored profile revision for {profile_id}."))
    }

    pub(crate) fn list(&self) -> Result<Vec<ProfileSummary>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare("SELECT DISTINCT profile_id FROM profile_revisions ORDER BY profile_id ASC")
            .map_err(|_| "AI-OS could not read its profile database.".to_owned())?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| "AI-OS could not list stored profiles.".to_owned())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "AI-OS could not list stored profiles.".to_owned())?;

        let mut summaries = Vec::new();
        for profile_id in ids {
            let history = self.history(&profile_id)?;
            let Some((latest, _)) = history.last() else {
                continue;
            };
            summaries.push(ProfileSummary {
                profile_id: profile_id.clone(),
                latest_revision: latest.revision,
                active_revision: history
                    .iter()
                    .rev()
                    .find(|(profile, _)| profile.status == ProfileStatus::Active)
                    .map(|(profile, _)| profile.revision),
                status: latest.status,
            });
        }
        Ok(summaries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive_distillation::review::test_support::{
        activate, bundle as make_bundle, drafted as make_drafted, review_everything,
    };

    fn store() -> (ProfileStore, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let store = ProfileStore::open_at(directory.path().join("profiles.sqlite3")).unwrap();
        (store, directory)
    }

    #[test]
    fn a_revision_survives_a_reopen_with_the_bundle_that_justifies_it() {
        let (store, directory) = store();
        let bundle = make_bundle("alice-bundle");
        let draft = make_drafted(&bundle);
        store.append(&draft, &bundle).unwrap();

        // A different handle, as a later app launch would be.
        let reopened = ProfileStore::open_at(directory.path().join("profiles.sqlite3")).unwrap();
        let (loaded, loaded_bundle) = reopened.latest("profile-alice").unwrap();
        assert_eq!(loaded, draft);
        assert_eq!(loaded_bundle.bundle_id, bundle.bundle_id);
        // Without the bundle the claims could not be re-verified at review time.
        assert_eq!(loaded_bundle.evidence.len(), bundle.evidence.len());
    }

    #[test]
    fn a_revision_cannot_be_stored_with_the_wrong_bundle() {
        let (store, _directory) = store();
        let first = make_bundle("alice-bundle");
        let draft = make_drafted(&first);
        assert!(store.append(&draft, &make_bundle("other-bundle")).is_err());
    }

    #[test]
    fn storage_is_append_only_so_history_is_never_lost() {
        let (store, _directory) = store();
        let bundle = make_bundle("alice-bundle");
        let draft = make_drafted(&bundle);
        store.append(&draft, &bundle).unwrap();

        let reviewed = review_everything(draft, &bundle);
        store.append(&reviewed, &bundle).unwrap();
        let active = activate(reviewed, &bundle);
        store.append(&active, &bundle).unwrap();

        let history = store.history("profile-alice").unwrap();
        // Three rows for one revision number: the draft is still readable after
        // review and activation replaced it.
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].0.status, ProfileStatus::Draft);
        assert_eq!(history[1].0.status, ProfileStatus::Reviewed);
        assert_eq!(history[2].0.status, ProfileStatus::Active);

        let ledger = store.ledger("profile-alice").unwrap();
        assert_eq!(ledger.active().unwrap().revision, 1);
    }

    #[test]
    fn listing_reports_the_latest_and_the_active_revision_separately() {
        let (store, _directory) = store();
        let bundle = make_bundle("alice-bundle");
        let draft = make_drafted(&bundle);
        let reviewed = review_everything(draft, &bundle);
        let active = activate(reviewed, &bundle);
        store.append(&active, &bundle).unwrap();

        // A second revision opened but not yet reviewed.
        let later = make_bundle("alice-bundle-2");
        let revised =
            crate::cognitive_distillation::review::revise(&active, &later, "more evidence")
                .unwrap();
        store.append(&revised, &later).unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        // The newest revision is a draft, but revision 1 is still the live one.
        assert_eq!(listed[0].latest_revision, 2);
        assert_eq!(listed[0].status, ProfileStatus::Draft);
        assert_eq!(listed[0].active_revision, Some(1));
    }

    #[test]
    fn an_unknown_profile_is_reported_rather_than_invented() {
        let (store, _directory) = store();
        assert!(store.latest("profile-nobody").is_err());
        assert!(store.history("profile-nobody").unwrap().is_empty());
        assert!(store.list().unwrap().is_empty());
    }
}

//! Shared data model, folder storage, migration, and vault encryption.
pub mod attachments;
pub mod audit;
pub mod backup;
pub mod credentials;
pub mod finance_settings;
pub mod journal;
pub mod library;
pub mod media;
pub mod model;
#[cfg(test)]
mod model_tests;
pub mod organizer;
pub mod recurrence;
pub mod reference_data;
pub mod squidy;
pub mod storage;
pub mod updates;

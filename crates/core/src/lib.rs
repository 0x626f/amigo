//! Shared data model, folder storage, migration, and vault encryption.
pub mod attachments;
pub mod audit;
pub mod backup;
pub mod credentials;
pub mod library;
pub mod model;
#[cfg(test)]
mod model_tests;
pub mod organizer;
pub mod recurrence;
pub mod reference_data;
pub mod storage;

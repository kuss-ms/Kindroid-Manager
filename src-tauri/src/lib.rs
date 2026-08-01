mod commands;
mod domain;
mod error;
mod kindroid;
mod security;
mod storage;

pub use commands::journal::{delete_journal_entry, list_journal_entries, save_journal_entry};
pub use commands::push::do_push;
pub use commands::share_code::{
    export_share_image, get_character_image, import_share_image, set_character_image,
};
pub use domain::character::Character;
pub use domain::image_share::{decode_image, encode_image, ImageShareError};
pub use domain::journal_entry::{JournalEntry, JournalEntryInput};
pub use domain::share_code::{
    decode as share_code_decode, encode as share_code_encode, PartialCharacter, ShareCodeError,
};
pub use kindroid::{
    ChatBreakRequest, HttpResponse, JournalCreateRequest, KindroidClient, KindroidError,
    UpdateInfoRequest,
};
pub use storage::sqlite::SqliteRepository;
pub use storage::{Repository, StorageError};

#[cfg(not(test))]
mod app;

#[cfg(not(test))]
pub use app::run;

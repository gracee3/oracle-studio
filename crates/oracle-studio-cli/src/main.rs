use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use chrono::{SecondsFormat, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use oracle_studio_app::{ManualPlacementInput, ReadingRequest, StudioService};
use oracle_studio_core::{
    ArtifactKind, JournalEntry, JournalEntryKind, PersonKind, PersonProfile, Session, StableId,
    VaultDocument,
};
use oracle_studio_storage::{ExpectedState, FileVault, LoadedVault, StorageError};
use sibylla_core::{Orientation, SpreadDefinition, SpreadLayout, SpreadPosition, UtcInstant};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(
    name = "oracle-studio",
    version,
    about = "Encrypted offline Oracle journal"
)]
struct Cli {
    #[arg(long)]
    vault: PathBuf,
    #[arg(long)]
    password_file: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init,
    PersonAdd {
        name: String,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        professional_client: bool,
        #[arg(long)]
        notes: Option<String>,
    },
    PersonUpdate {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long, value_enum)]
        kind: Option<PersonType>,
    },
    PersonList,
    SessionAdd {
        title: String,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        person: Option<String>,
        #[arg(long)]
        context: Option<String>,
    },
    SessionUpdate {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        context: Option<String>,
    },
    SessionList,
    DeckImport {
        file: PathBuf,
        #[arg(long)]
        id: Option<String>,
    },
    DeckList,
    ChartImport {
        file: PathBuf,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        person: Option<String>,
        #[arg(long)]
        session: Option<String>,
    },
    ReadingNew {
        #[arg(long)]
        deck: String,
        #[arg(long)]
        person: Option<String>,
        #[arg(long)]
        session: Option<String>,
        #[arg(long, value_enum)]
        method: ReadingMethod,
    },
    Annotate {
        content: String,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        person: Option<String>,
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        artifact: Option<String>,
        #[arg(long, value_enum, default_value_t = EntryKind::Annotation)]
        kind: EntryKind,
    },
    Search {
        query: String,
    },
    BackupExport {
        destination: PathBuf,
    },
    BackupImport {
        source: PathBuf,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ReadingMethod {
    Manual,
    Software,
}

#[derive(Clone, Copy, ValueEnum)]
enum EntryKind {
    Annotation,
    Outcome,
}

#[derive(Clone, Copy, ValueEnum)]
enum PersonType {
    Personal,
    ProfessionalClient,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    let repository = FileVault::new(cli.vault)?;
    match cli.command {
        Command::Init => {
            let password = read_password(cli.password_file.as_deref(), true)?;
            repository.save(&VaultDocument::empty(), &password, &ExpectedState::Missing)?;
            println!(
                "Initialized encrypted vault at {}",
                repository.path().display()
            );
        }
        Command::BackupExport { destination } => {
            let password = read_password(cli.password_file.as_deref(), false)?;
            let revision = repository.export_backup(destination, &password)?;
            println!("Exported authenticated backup {}", revision.as_str());
        }
        Command::BackupImport { source } => {
            let password = read_password(cli.password_file.as_deref(), false)?;
            let expected = current_expectation(&repository, &password)?;
            let imported = repository.import_backup(source, &password, &expected)?;
            println!(
                "Imported authenticated backup {}",
                imported.revision().as_str()
            );
        }
        command => {
            let password = read_password(cli.password_file.as_deref(), false)?;
            let loaded = repository.load(&password)?;
            dispatch(command, &repository, &password, loaded)?;
        }
    }
    Ok(())
}

fn dispatch(
    command: Command,
    repository: &FileVault,
    password: &[u8],
    loaded: LoadedVault,
) -> Result<(), CliError> {
    let document = loaded.document();
    let changed = match command {
        Command::PersonAdd {
            name,
            id,
            professional_client,
            notes,
        } => Some(StudioService::add_person(
            document,
            PersonProfile::new(
                app_id("person.id", id)?,
                name,
                if professional_client {
                    PersonKind::ProfessionalClient
                } else {
                    PersonKind::Personal
                },
                notes,
            )?,
        )?),
        Command::PersonUpdate {
            id,
            name,
            notes,
            kind,
        } => {
            let id = StableId::new("person.id", id)?;
            let old = document
                .people()
                .iter()
                .find(|person| person.id() == &id)
                .ok_or(CliError::NotFound("person"))?;
            Some(StudioService::replace_person(
                document,
                PersonProfile::new(
                    id,
                    name.unwrap_or_else(|| old.display_name().to_owned()),
                    kind.map_or(old.kind(), |kind| match kind {
                        PersonType::Personal => PersonKind::Personal,
                        PersonType::ProfessionalClient => PersonKind::ProfessionalClient,
                    }),
                    notes.or_else(|| old.notes().map(str::to_owned)),
                )?,
            )?)
        }
        Command::PersonList => {
            for person in document.people() {
                println!(
                    "{}\t{:?}\t{}",
                    person.id().as_str(),
                    person.kind(),
                    person.display_name()
                );
            }
            None
        }
        Command::SessionAdd {
            title,
            id,
            person,
            context,
        } => {
            let now = now();
            Some(StudioService::add_session(
                document,
                Session::new(
                    app_id("session.id", id)?,
                    optional_id("session.person_id", person)?,
                    title,
                    context,
                    &now,
                    &now,
                )?,
            )?)
        }
        Command::SessionUpdate { id, title, context } => {
            let id = StableId::new("session.id", id)?;
            let old = document
                .sessions()
                .iter()
                .find(|session| session.id() == &id)
                .ok_or(CliError::NotFound("session"))?;
            Some(StudioService::replace_session(
                document,
                Session::new(
                    id,
                    old.person_id().cloned(),
                    title.unwrap_or_else(|| old.title().to_owned()),
                    context.or_else(|| old.context().map(str::to_owned)),
                    old.created_at(),
                    now(),
                )?,
            )?)
        }
        Command::SessionList => {
            for session in document.sessions() {
                println!("{}\t{}", session.id().as_str(), session.title());
            }
            None
        }
        Command::DeckImport { file, id } => Some(StudioService::import_deck(
            document,
            app_id("artifact.id", id)?,
            &fs::read_to_string(file)?,
        )?),
        Command::DeckList => {
            for artifact in document
                .artifacts()
                .iter()
                .filter(|artifact| artifact.kind() == ArtifactKind::SibyllaDeck)
            {
                let deck = StudioService::deck_manifest(document, artifact.id())?;
                println!("{}\t{}", artifact.id().as_str(), deck.name());
            }
            None
        }
        Command::ChartImport {
            file,
            id,
            person,
            session,
        } => Some(StudioService::import_chart(
            document,
            app_id("artifact.id", id)?,
            optional_id("artifact.person_id", person)?,
            optional_id("artifact.session_id", session)?,
            &fs::read_to_string(file)?,
        )?),
        Command::ReadingNew {
            deck,
            person,
            session,
            method,
        } => Some(reading_wizard(document, deck, person, session, method)?),
        Command::Annotate {
            content,
            id,
            person,
            session,
            artifact,
            kind,
        } => Some(StudioService::add_journal_entry(
            document,
            JournalEntry::new(
                app_id("journal_entry.id", id)?,
                optional_id("journal_entry.person_id", person)?,
                optional_id("journal_entry.session_id", session)?,
                optional_id("journal_entry.artifact_id", artifact)?,
                match kind {
                    EntryKind::Annotation => JournalEntryKind::Annotation,
                    EntryKind::Outcome => JournalEntryKind::Outcome,
                },
                content,
                now(),
            )?,
        )?),
        Command::Search { query } => {
            for hit in StudioService::search(document, &query)? {
                println!(
                    "{:?}\t{}\t{}",
                    hit.entity(),
                    hit.id().as_str(),
                    hit.snippet()
                );
            }
            None
        }
        Command::Init | Command::BackupExport { .. } | Command::BackupImport { .. } => {
            unreachable!("handled before vault loading")
        }
    };
    if let Some(changed) = changed {
        let revision = repository.save(
            &changed,
            password,
            &ExpectedState::Revision(loaded.revision().clone()),
        )?;
        println!("Saved encrypted vault {}", revision.as_str());
    }
    Ok(())
}

fn reading_wizard(
    document: &VaultDocument,
    deck_id: String,
    person: Option<String>,
    session: Option<String>,
    method: ReadingMethod,
) -> Result<VaultDocument, CliError> {
    let deck_id = StableId::new("deck_record_id", deck_id)?;
    let deck = StudioService::deck_manifest(document, &deck_id)?;
    println!("Deck: {}", deck.name());
    let spread = guided_spread()?;
    let person_id = optional_id("reading.person_id", person)?;
    let session_id = optional_id("reading.session_id", session)?;
    let timestamp = UtcInstant::parse_rfc3339(&now())?;
    let request = ReadingRequest {
        artifact_record_id: app_id("artifact.id", None)?,
        reading_id: format!("reading_{}", Uuid::now_v7().simple()),
        deck_record_id: deck_id,
        person_id,
        session_id,
        spread,
        question: prompt_optional("Question/intention")?,
        context: prompt_optional("Background/context")?,
        reader_notes: prompt_optional("Reader notes")?,
        interpretation: prompt_optional("Interpretation")?,
        timestamp,
    };
    let changed = match method {
        ReadingMethod::Manual => {
            println!("Enabled cards:");
            for card in deck.enabled_cards() {
                println!("  {} — {}", card.id().as_str(), card.printed_title());
            }
            let mut placements = Vec::new();
            for position in request.spread.positions() {
                println!("Position: {}", position.label());
                let deck_card_id = prompt_required("Deck card ID")?;
                let orientation =
                    match prompt_default("Orientation (upright/reversed/unspecified)", "upright")?
                        .as_str()
                    {
                        "upright" => Orientation::Upright,
                        "reversed" => Orientation::Reversed,
                        "unspecified" => Orientation::Unspecified,
                        _ => return Err(CliError::InvalidInput("orientation")),
                    };
                placements.push(ManualPlacementInput {
                    deck_card_id,
                    orientation,
                    notes: prompt_optional("Card notes")?,
                });
            }
            StudioService::record_manual_reading(document, request, placements)?
        }
        ReadingMethod::Software => StudioService::record_software_reading(document, request)?,
    };
    let record = changed
        .artifacts()
        .last()
        .ok_or(CliError::NotFound("reading"))?;
    let artifact = sibylla_artifacts::ReadingArtifact::from_json(record.canonical_json())?;
    println!("Reading preview:");
    for placement in artifact.payload().placements() {
        println!(
            "  {}: {} ({:?})",
            placement.position_label(),
            placement.printed_title(),
            placement.orientation()
        );
    }
    if prompt_default("Save this reading? (yes/no)", "no")? != "yes" {
        return Err(CliError::Cancelled);
    }
    Ok(changed)
}

fn guided_spread() -> Result<SpreadDefinition, CliError> {
    match prompt_default("Spread (one/three/freeform)", "one")?.as_str() {
        "one" => Ok(SpreadDefinition::one_card()),
        "three" => {
            let defaults = ["Situation", "Tension", "Next Step"];
            let mut positions = Vec::new();
            for (index, default) in defaults.iter().enumerate() {
                positions.push(SpreadPosition::new(
                    sibylla_core::StableId::new("position_id", format!("position_{}", index + 1))?,
                    prompt_default(&format!("Position {} label", index + 1), default)?,
                    prompt_optional("Position meaning")?,
                    None,
                )?);
            }
            Ok(SpreadDefinition::three_card(
                sibylla_core::StableId::new("spread_id", "guided_three_card")?,
                "Guided Three Card",
                positions
                    .try_into()
                    .map_err(|_| CliError::InvalidInput("spread"))?,
            )?)
        }
        "freeform" => {
            let count: usize = prompt_required("Number of positions")?
                .parse()
                .map_err(|_| CliError::InvalidInput("position count"))?;
            let mut positions = Vec::new();
            for index in 0..count {
                positions.push(SpreadPosition::new(
                    sibylla_core::StableId::new("position_id", format!("position_{}", index + 1))?,
                    prompt_required(&format!("Position {} label", index + 1))?,
                    prompt_optional("Position meaning")?,
                    None,
                )?);
            }
            Ok(SpreadDefinition::new(
                sibylla_core::StableId::new("spread_id", format!("guided_freeform_{count}"))?,
                "Guided Freeform",
                SpreadLayout::Freeform,
                positions,
            )?)
        }
        _ => Err(CliError::InvalidInput("spread")),
    }
}

fn current_expectation(repository: &FileVault, password: &[u8]) -> Result<ExpectedState, CliError> {
    match repository.load(password) {
        Ok(loaded) => Ok(ExpectedState::Revision(loaded.revision().clone())),
        Err(StorageError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            Ok(ExpectedState::Missing)
        }
        Err(error) => Err(error.into()),
    }
}

fn read_password(path: Option<&Path>, confirm: bool) -> Result<Zeroizing<Vec<u8>>, CliError> {
    if let Some(path) = path {
        validate_password_file(path)?;
        let bytes = fs::read(path)?;
        let password = trim_line_endings(bytes);
        if password.is_empty() {
            return Err(CliError::InvalidInput("empty password file"));
        }
        return Ok(Zeroizing::new(password));
    }
    let password = Zeroizing::new(rpassword::prompt_password("Vault password: ")?.into_bytes());
    if password.is_empty() {
        return Err(CliError::InvalidInput("empty password"));
    }
    if confirm {
        let repeated =
            Zeroizing::new(rpassword::prompt_password("Confirm password: ")?.into_bytes());
        if *password != *repeated {
            return Err(CliError::PasswordMismatch);
        }
    }
    Ok(password)
}

#[cfg(unix)]
fn validate_password_file(path: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt;
    if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
        return Err(CliError::InsecurePasswordFile);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_password_file(_path: &Path) -> Result<(), CliError> {
    Ok(())
}

fn trim_line_endings(mut bytes: Vec<u8>) -> Vec<u8> {
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    bytes
}

fn prompt_required(label: &str) -> Result<String, CliError> {
    let value = prompt(label)?;
    if value.trim().is_empty() {
        Err(CliError::InvalidInput("blank response"))
    } else {
        Ok(value.trim().to_owned())
    }
}

fn prompt_optional(label: &str) -> Result<Option<String>, CliError> {
    let value = prompt(label)?;
    let value = value.trim();
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

fn prompt_default(label: &str, default: &str) -> Result<String, CliError> {
    let value = prompt(&format!("{label} [{default}]"))?;
    Ok(if value.trim().is_empty() {
        default.to_owned()
    } else {
        value.trim().to_owned()
    })
}

fn prompt(label: &str) -> Result<String, CliError> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut value = String::new();
    if io::stdin().read_line(&mut value)? == 0 {
        return Err(CliError::Cancelled);
    }
    Ok(value)
}

fn app_id(field: &'static str, supplied: Option<String>) -> Result<StableId, CliError> {
    Ok(StableId::new(
        field,
        supplied.unwrap_or_else(|| Uuid::now_v7().to_string()),
    )?)
}

fn optional_id(
    field: &'static str,
    supplied: Option<String>,
) -> Result<Option<StableId>, CliError> {
    supplied
        .map(|id| StableId::new(field, id))
        .transpose()
        .map_err(Into::into)
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[derive(Debug, Error)]
enum CliError {
    #[error("operation cancelled; vault was not changed")]
    Cancelled,
    #[error("password confirmation did not match")]
    PasswordMismatch,
    #[error("password file permissions allow access outside the owner")]
    InsecurePasswordFile,
    #[error("invalid {0}")]
    InvalidInput(&'static str),
    #[error("{0} was not found")]
    NotFound(&'static str),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Model(#[from] oracle_studio_core::ModelError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    App(#[from] oracle_studio_app::AppError),
    #[error(transparent)]
    Sibylla(#[from] sibylla_core::ValidationError),
    #[error(transparent)]
    Artifact(#[from] sibylla_artifacts::ArtifactError),
}

use crate::service::HubError;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Map, Value};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

const HEADER_LEN: usize = 16;
const RECORD_LEN: usize = 88;
const MAX_META_LEN: usize = 1024 * 1024;
const INDEX_SCHEMA_VERSION: &str = "1";

#[derive(Clone)]
struct Record {
    sequence: u64,
    kind: u32,
    thread_id: u32,
    address: u64,
    args: [u64; 8],
}

struct DatabaseInfo {
    path: PathBuf,
    source_bytes: u64,
    physical_records: u64,
    truncated_tail: usize,
    metadata_json: String,
}

pub fn query(path: &str, args: &Map<String, Value>) -> Result<Value, HubError> {
    let index = required_string(args, "index")?;
    let key = required_string(args, "key")?;
    let limit = required_string(args, "limit")?
        .parse::<usize>()
        .map_err(|_| HubError::Validation("Trace index limit must be decimal".into()))?;
    if !(1..=256).contains(&limit) {
        return Err(HubError::Validation(
            "Trace index limit must be 1..256".into(),
        ));
    }
    let before = args
        .get("before")
        .and_then(Value::as_str)
        .map(parse_integer)
        .transpose()?
        .unwrap_or(0);
    let payload = args
        .get("payload")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let fields = parse_fields(args)?;
    let predicate = Predicate::parse(index, key)?;
    let include_metadata = args
        .get("metadata")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let database = ensure_database(path)?;
    let connection = Connection::open(&database.path)
        .map_err(|error| HubError::Internal(format!("open Trace database: {error}")))?;
    let (matched_total, eligible_total, mut records) =
        query_database(&connection, &predicate, before, limit)?;
    records.reverse();
    let first_sequence = records.first().map(|record| record.sequence).unwrap_or(0);
    let events = records
        .iter()
        .map(|record| project_record(record, payload, &fields))
        .collect::<Vec<_>>();
    let has_older = eligible_total > events.len() as u64;
    let next_before = if has_older {
        first_sequence.to_string()
    } else {
        "0".into()
    };
    let metadata = if include_metadata {
        Some(
            serde_json::from_str::<Value>(&database.metadata_json).map_err(|error| {
                HubError::Validation(format!("invalid Trace metadata: {error}"))
            })?,
        )
    } else {
        None
    };
    Ok(json!({
        "artifact": path,
        "database": database.path.to_string_lossy(),
        "file_bytes": database.source_bytes.to_string(),
        "format": "pbtr",
        "version": "1",
        "query_store": "sqlite",
        "index": index,
        "key": key,
        "before": before.to_string(),
        "limit": limit.to_string(),
        "payload": payload,
        "physical_records": database.physical_records.to_string(),
        "matched_total": matched_total.to_string(),
        "eligible_total": eligible_total.to_string(),
        "returned": events.len().to_string(),
        "has_older": has_older,
        "next_before": next_before,
        "truncated_tail_bytes": database.truncated_tail.to_string(),
        "metadata": metadata,
        "events": events,
    }))
}

pub fn prepare(path: &str) -> Result<Value, HubError> {
    let database = ensure_database(path)?;
    let database_bytes = std::fs::metadata(&database.path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    Ok(json!({
        "state": "ready",
        "query_store": "sqlite",
        "database": database.path.to_string_lossy(),
        "database_bytes": database_bytes.to_string(),
        "records": database.physical_records.to_string(),
    }))
}

fn query_database(
    connection: &Connection,
    predicate: &Predicate,
    before: u64,
    limit: usize,
) -> Result<(u64, u64, Vec<Record>), HubError> {
    let (from_sql, predicate_sql, key) = predicate.database_filter()?;
    let matched_sql = format!("SELECT COUNT(*) {from_sql} WHERE {predicate_sql}");
    let matched_total = connection
        .query_row(&matched_sql, params![key], |row| row.get::<_, i64>(0))
        .map_err(|error| HubError::Internal(format!("count Trace records: {error}")))?;
    let matched_total = u64::try_from(matched_total)
        .map_err(|_| HubError::Internal("Trace database returned a negative count".into()))?;
    let before = if before == 0 {
        None
    } else {
        Some(
            i64::try_from(before)
                .map_err(|_| HubError::Validation("Trace page position is out of range".into()))?,
        )
    };
    let eligible_total = if let Some(before) = before {
        let sql = format!("SELECT COUNT(*) {from_sql} WHERE {predicate_sql} AND e.sequence < ?2");
        connection
            .query_row(&sql, params![key, before], |row| row.get::<_, i64>(0))
            .map_err(|error| HubError::Internal(format!("count Trace page: {error}")))?
            .try_into()
            .map_err(|_| HubError::Internal("Trace database returned a negative count".into()))?
    } else {
        matched_total
    };
    let select = format!(
        "SELECT e.sequence,e.kind,e.thread_id,e.address,e.payload {from_sql} WHERE {predicate_sql}{} ORDER BY e.sequence DESC LIMIT {}",
        if before.is_some() { " AND e.sequence < ?2" } else { "" },
        if before.is_some() { "?3" } else { "?2" },
    );
    let mut statement = connection
        .prepare(&select)
        .map_err(|error| HubError::Internal(format!("prepare Trace query: {error}")))?;
    let records = if let Some(before) = before {
        statement
            .query_map(params![key, before, limit as i64], row_to_record)
            .map_err(|error| HubError::Internal(format!("query Trace records: {error}")))?
            .collect::<Result<Vec<_>, _>>()
    } else {
        statement
            .query_map(params![key, limit as i64], row_to_record)
            .map_err(|error| HubError::Internal(format!("query Trace records: {error}")))?
            .collect::<Result<Vec<_>, _>>()
    }
    .map_err(|error| HubError::Internal(format!("read Trace records: {error}")))?;
    Ok((matched_total, eligible_total, records))
}

fn row_to_record(row: &Row<'_>) -> rusqlite::Result<Record> {
    let sequence = row.get::<_, i64>(0)?;
    let kind = row.get::<_, i64>(1)?;
    let thread_id = row.get::<_, i64>(2)?;
    let address = row.get::<_, i64>(3)?;
    let payload = row.get::<_, Vec<u8>>(4)?;
    if payload.len() != 64
        || sequence < 0
        || !(0..=u32::MAX as i64).contains(&kind)
        || !(0..=u32::MAX as i64).contains(&thread_id)
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let mut args = [0u64; 8];
    for (index, value) in args.iter_mut().enumerate() {
        let offset = index * 8;
        *value = read_u64(&payload[offset..offset + 8]);
    }
    Ok(Record {
        sequence: sequence as u64,
        kind: kind as u32,
        thread_id: thread_id as u32,
        address: address as u64,
        args,
    })
}

fn ensure_database(path: &str) -> Result<DatabaseInfo, HubError> {
    let source = Path::new(path);
    let metadata = std::fs::metadata(source)
        .map_err(|error| HubError::Validation(format!("Trace artifact is unavailable: {error}")))?;
    let source_bytes = metadata.len();
    let source_modified = modified_stamp(&metadata)?;
    let database = sidecar_path(source);
    if let Ok(Some(info)) = database_info(&database, source_bytes, &source_modified) {
        return Ok(info);
    }
    build_database(source, &database, source_bytes, &source_modified)
}

fn build_database(
    source: &Path,
    database: &Path,
    source_bytes: u64,
    source_modified: &str,
) -> Result<DatabaseInfo, HubError> {
    let (mut reader, metadata_json) = open_trace(source)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| HubError::Internal(error.to_string()))?
        .as_nanos();
    let temporary = PathBuf::from(format!(
        "{}.{}-{nonce}.tmp",
        database.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut connection = Connection::open(&temporary)
            .map_err(|error| HubError::Internal(format!("create Trace database: {error}")))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=OFF;
                 PRAGMA synchronous=OFF;
                 PRAGMA temp_store=MEMORY;
                 PRAGMA locking_mode=EXCLUSIVE;
                 CREATE TABLE trace_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE events(
                    sequence INTEGER PRIMARY KEY,
                    kind INTEGER NOT NULL,
                    thread_id INTEGER NOT NULL,
                    address INTEGER NOT NULL,
                    payload BLOB NOT NULL
                 );
                 CREATE TABLE memory_index(
                    sequence INTEGER PRIMARY KEY,
                    memory_address INTEGER NOT NULL,
                    FOREIGN KEY(sequence) REFERENCES events(sequence)
                 );",
            )
            .map_err(|error| HubError::Internal(format!("initialize Trace database: {error}")))?;
        let mut physical_records = 0u64;
        let mut truncated_tail = 0usize;
        {
            let transaction = connection
                .transaction()
                .map_err(|error| HubError::Internal(format!("begin Trace import: {error}")))?;
            {
                let mut insert_event = transaction
                    .prepare(
                        "INSERT INTO events(sequence,kind,thread_id,address,payload) VALUES(?1,?2,?3,?4,?5)",
                    )
                    .map_err(|error| HubError::Internal(format!("prepare Trace import: {error}")))?;
                let mut insert_memory = transaction
                    .prepare("INSERT INTO memory_index(sequence,memory_address) VALUES(?1,?2)")
                    .map_err(|error| {
                        HubError::Internal(format!("prepare memory index: {error}"))
                    })?;
                loop {
                    let mut bytes = [0u8; RECORD_LEN];
                    let mut filled = 0usize;
                    while filled < RECORD_LEN {
                        let count = reader
                            .read(&mut bytes[filled..])
                            .map_err(|error| HubError::Internal(error.to_string()))?;
                        if count == 0 {
                            break;
                        }
                        filled += count;
                    }
                    if filled == 0 {
                        break;
                    }
                    if filled != RECORD_LEN {
                        truncated_tail = filled;
                        break;
                    }
                    let record = parse_record(&bytes);
                    let sequence = i64::try_from(record.sequence).map_err(|_| {
                        HubError::Validation(
                            "Trace sequence exceeds the local database range".into(),
                        )
                    })?;
                    insert_event
                        .execute(params![
                            sequence,
                            record.kind as i64,
                            record.thread_id as i64,
                            record.address as i64,
                            &bytes[24..],
                        ])
                        .map_err(|error| {
                            HubError::Internal(format!("import Trace event: {error}"))
                        })?;
                    if matches!(record.kind, 2 | 10) {
                        insert_memory
                            .execute(params![sequence, record.args[0] as i64])
                            .map_err(|error| {
                                HubError::Internal(format!("import Trace memory index: {error}"))
                            })?;
                    }
                    physical_records += 1;
                }
            }
            transaction
                .commit()
                .map_err(|error| HubError::Internal(format!("commit Trace import: {error}")))?;
        }
        connection
            .execute_batch(
                "CREATE INDEX events_kind_sequence ON events(kind,sequence DESC);
                 CREATE INDEX events_thread_sequence ON events(thread_id,sequence DESC);
                 CREATE INDEX events_address_sequence ON events(address,sequence DESC);
                 CREATE INDEX memory_address_sequence ON memory_index(memory_address,sequence DESC);",
            )
            .map_err(|error| HubError::Internal(format!("build Trace indexes: {error}")))?;
        for (key, value) in [
            ("schema_version", INDEX_SCHEMA_VERSION.to_string()),
            ("source_bytes", source_bytes.to_string()),
            ("source_modified", source_modified.to_string()),
            ("physical_records", physical_records.to_string()),
            ("truncated_tail", truncated_tail.to_string()),
            ("metadata_json", metadata_json.clone()),
        ] {
            connection
                .execute(
                    "INSERT INTO trace_meta(key,value) VALUES(?1,?2)",
                    params![key, value],
                )
                .map_err(|error| {
                    HubError::Internal(format!("write Trace index metadata: {error}"))
                })?;
        }
        connection
            .execute_batch("PRAGMA optimize;")
            .map_err(|error| HubError::Internal(format!("optimize Trace database: {error}")))?;
        drop(connection);

        let current_metadata = std::fs::metadata(source)
            .map_err(|error| HubError::Validation(format!("Trace artifact changed: {error}")))?;
        if current_metadata.len() != source_bytes
            || modified_stamp(&current_metadata)? != source_modified
        {
            return Err(HubError::Validation(
                "Trace recording changed while its local database was being built".into(),
            ));
        }
        if database.exists() {
            std::fs::remove_file(database).map_err(|error| {
                HubError::Internal(format!("replace stale Trace database: {error}"))
            })?;
        }
        std::fs::rename(&temporary, database)
            .map_err(|error| HubError::Internal(format!("publish Trace database: {error}")))?;
        database_info(database, source_bytes, source_modified)?
            .ok_or_else(|| HubError::Internal("new Trace database failed validation".into()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn open_trace(source: &Path) -> Result<(BufReader<File>, String), HubError> {
    let file = File::open(source)
        .map_err(|error| HubError::Validation(format!("Trace artifact is unavailable: {error}")))?;
    let mut reader = BufReader::new(file);
    let mut header = [0u8; HEADER_LEN];
    reader
        .read_exact(&mut header)
        .map_err(|_| HubError::Validation("Trace artifact has a short header".into()))?;
    if &header[..4] != b"PBTR" {
        return Err(HubError::Validation(
            "Trace artifact has invalid PBTR magic".into(),
        ));
    }
    let version = read_u32(&header[4..8]);
    if version != 1 {
        return Err(HubError::Validation(format!(
            "unsupported PBTR version: {version}"
        )));
    }
    let metadata_len = read_u32(&header[8..12]) as usize;
    if metadata_len > MAX_META_LEN {
        return Err(HubError::Validation("Trace metadata exceeds 1 MiB".into()));
    }
    let mut metadata = vec![0u8; metadata_len];
    reader
        .read_exact(&mut metadata)
        .map_err(|_| HubError::Validation("Trace artifact has truncated metadata".into()))?;
    let metadata_json = if metadata.is_empty() {
        "{}".to_string()
    } else {
        serde_json::from_slice::<Value>(&metadata)
            .map_err(|error| HubError::Validation(format!("invalid Trace metadata: {error}")))?;
        String::from_utf8(metadata)
            .map_err(|error| HubError::Validation(format!("invalid Trace metadata: {error}")))?
    };
    Ok((reader, metadata_json))
}

fn database_info(
    database: &Path,
    source_bytes: u64,
    source_modified: &str,
) -> Result<Option<DatabaseInfo>, HubError> {
    if !database.is_file() {
        return Ok(None);
    }
    let connection = Connection::open(database)
        .map_err(|error| HubError::Internal(format!("open Trace database: {error}")))?;
    let schema_version = meta_value(&connection, "schema_version")?;
    let indexed_bytes = meta_value(&connection, "source_bytes")?;
    let indexed_modified = meta_value(&connection, "source_modified")?;
    if schema_version.as_deref() != Some(INDEX_SCHEMA_VERSION)
        || indexed_bytes.as_deref() != Some(source_bytes.to_string().as_str())
        || indexed_modified.as_deref() != Some(source_modified)
    {
        return Ok(None);
    }
    let physical_records = meta_value(&connection, "physical_records")?
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| HubError::Internal("Trace database has no record count".into()))?;
    let truncated_tail = meta_value(&connection, "truncated_tail")?
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let metadata_json = meta_value(&connection, "metadata_json")?.unwrap_or_else(|| "{}".into());
    Ok(Some(DatabaseInfo {
        path: database.to_path_buf(),
        source_bytes,
        physical_records,
        truncated_tail,
        metadata_json,
    }))
}

fn meta_value(connection: &Connection, key: &str) -> Result<Option<String>, HubError> {
    connection
        .query_row(
            "SELECT value FROM trace_meta WHERE key=?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| HubError::Internal(format!("read Trace database metadata: {error}")))
}

fn sidecar_path(source: &Path) -> PathBuf {
    PathBuf::from(format!("{}.sqlite", source.to_string_lossy()))
}

fn modified_stamp(metadata: &std::fs::Metadata) -> Result<String, HubError> {
    metadata
        .modified()
        .map_err(|error| HubError::Internal(error.to_string()))?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .map_err(|error| HubError::Internal(error.to_string()))
}

pub fn export(path: &str, args: &Map<String, Value>) -> Result<Value, HubError> {
    let result = query(path, args)?;
    let events = result
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| HubError::Internal("Trace index result has no events".into()))?;
    let format = args
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("jsonl");
    let (extension, mime_type, data) = match format {
        "json" => (
            "json",
            "application/json",
            serde_json::to_string_pretty(events)
                .map_err(|error| HubError::Internal(error.to_string()))?,
        ),
        "jsonl" => (
            "jsonl",
            "application/x-ndjson",
            events
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| HubError::Internal(error.to_string()))?
                .join("\n"),
        ),
        "csv" => ("csv", "text/csv", csv(events)),
        _ => {
            return Err(HubError::Validation(
                "Trace index export format must be json, jsonl, or csv".into(),
            ))
        }
    };
    let requested = args
        .get("filename")
        .and_then(Value::as_str)
        .unwrap_or("trace-index");
    let filename = safe_filename(requested, extension)?;
    match args
        .get("delivery")
        .and_then(Value::as_str)
        .unwrap_or("file")
    {
        "inline" => {
            if data.len() > 2 * 1024 * 1024 {
                return Err(HubError::Validation(
                    "inline Trace export exceeds 2 MiB; use delivery=file".into(),
                ));
            }
            Ok(json!({
                "delivery":"inline",
                "format":format,
                "mime_type":mime_type,
                "filename":filename,
                "rows":events.len().to_string(),
                "bytes":data.len().to_string(),
                "data":data,
            }))
        }
        "file" => {
            let directory = std::env::temp_dir().join("pinbridge-trace-exports");
            std::fs::create_dir_all(&directory)
                .map_err(|error| HubError::Internal(error.to_string()))?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| HubError::Internal(error.to_string()))?
                .as_millis();
            let output = directory.join(format!("{stamp}-{}-{filename}", std::process::id()));
            std::fs::write(&output, data.as_bytes())
                .map_err(|error| HubError::Internal(error.to_string()))?;
            Ok(json!({
                "delivery":"file",
                "format":format,
                "mime_type":mime_type,
                "filename":filename,
                "path":output.to_string_lossy(),
                "rows":events.len().to_string(),
                "bytes":data.len().to_string(),
            }))
        }
        _ => Err(HubError::Validation(
            "Trace index export delivery must be file or inline".into(),
        )),
    }
}

enum Predicate {
    Kind(u32),
    Address(u64),
    Thread(u32),
    Sequence(u64),
    Memory(u64),
}

impl Predicate {
    fn parse(index: &str, key: &str) -> Result<Self, HubError> {
        match index {
            "kind" => Ok(Self::Kind(parse_kind(key)?)),
            "address" => Ok(Self::Address(parse_integer(key)?)),
            "thread" => Ok(Self::Thread(u32::try_from(parse_integer(key)?).map_err(
                |_| HubError::Validation("Trace thread id is out of range".into()),
            )?)),
            "sequence" => Ok(Self::Sequence(parse_integer(key)?)),
            "memory" => Ok(Self::Memory(parse_integer(key)?)),
            _ => Err(HubError::Validation(
                "Trace index must be kind, address, thread, sequence, or memory".into(),
            )),
        }
    }

    fn database_filter(&self) -> Result<(&'static str, &'static str, i64), HubError> {
        match self {
            Self::Kind(kind) => Ok(("FROM events e", "e.kind=?1", *kind as i64)),
            Self::Address(address) => Ok(("FROM events e", "e.address=?1", *address as i64)),
            Self::Thread(thread) => Ok(("FROM events e", "e.thread_id=?1", *thread as i64)),
            Self::Sequence(sequence) => Ok((
                "FROM events e",
                "e.sequence=?1",
                i64::try_from(*sequence)
                    .map_err(|_| HubError::Validation("Trace sequence is out of range".into()))?,
            )),
            Self::Memory(address) => Ok((
                "FROM memory_index m JOIN events e ON e.sequence=m.sequence",
                "m.memory_address=?1",
                *address as i64,
            )),
        }
    }
}

fn parse_record(bytes: &[u8; RECORD_LEN]) -> Record {
    let mut args = [0u64; 8];
    for (index, value) in args.iter_mut().enumerate() {
        let offset = 24 + index * 8;
        *value = read_u64(&bytes[offset..offset + 8]);
    }
    Record {
        sequence: read_u64(&bytes[0..8]),
        kind: read_u32(&bytes[8..12]),
        thread_id: read_u32(&bytes[12..16]),
        address: read_u64(&bytes[16..24]),
        args,
    }
}

fn project_record(record: &Record, payload: bool, fields: &[String]) -> Value {
    let mut row = Map::new();
    row.insert("sequence".into(), json!(record.sequence.to_string()));
    row.insert("kind".into(), json!(kind_name(record.kind)));
    row.insert("kind_id".into(), json!(record.kind.to_string()));
    row.insert("thread_id".into(), json!(record.thread_id.to_string()));
    row.insert("address".into(), json!(format!("0x{:x}", record.address)));
    if payload {
        add_payload(&mut row, record);
    }
    if fields.is_empty() {
        if !payload {
            row.remove("kind_id");
        }
        return Value::Object(row);
    }
    if fields.iter().any(|field| !row.contains_key(field)) {
        add_payload(&mut row, record);
    }
    let projected = fields
        .iter()
        .filter_map(|field| row.get(field).cloned().map(|value| (field.clone(), value)))
        .collect();
    Value::Object(projected)
}

fn add_payload(row: &mut Map<String, Value>, record: &Record) {
    match record.kind {
        2 | 10 => {
            row.insert("memory".into(), json!(format!("0x{:x}", record.args[0])));
            row.insert("size".into(), json!(record.args[1].to_string()));
            row.insert(
                "access".into(),
                json!(match record.args[2] {
                    0 => "read",
                    1 => "write",
                    2 => "read2",
                    _ => "unknown",
                }),
            );
            if record.kind == 10 {
                row.insert("value".into(), json!(format!("0x{:x}", record.args[3])));
            }
        }
        3 => {
            row.insert("size".into(), json!(record.args[0].to_string()));
        }
        4 => {
            row.insert("target".into(), json!(format!("0x{:x}", record.args[0])));
            row.insert("taken".into(), json!(record.args[1] != 0));
        }
        5 => {
            let exit = record.args[1] != 0;
            row.insert("number".into(), json!(record.args[0].to_string()));
            row.insert("phase".into(), json!(if exit { "exit" } else { "entry" }));
            if exit {
                row.insert(
                    "return_value".into(),
                    json!(format!("0x{:x}", record.args[3])),
                );
                row.insert("errno".into(), json!(format!("0x{:x}", record.args[4])));
            } else {
                row.insert(
                    "arguments".into(),
                    json!(record.args[2..]
                        .iter()
                        .map(|value| format!("0x{value:x}"))
                        .collect::<Vec<_>>()),
                );
            }
        }
        6 => {
            row.insert("reason".into(), json!(record.args[0].to_string()));
            row.insert("info".into(), json!(format!("0x{:x}", record.args[1])));
            row.insert(
                "context_ip".into(),
                json!(format!("0x{:x}", record.args[2])),
            );
        }
        9 => {
            let size = record.args[0].min(15) as usize;
            let mut bytes = Vec::with_capacity(16);
            bytes.extend_from_slice(&record.args[1].to_le_bytes());
            bytes.extend_from_slice(&record.args[2].to_le_bytes());
            row.insert("size".into(), json!(record.args[0].to_string()));
            row.insert("bytes".into(), json!(hex_bytes(&bytes[..size])));
        }
        11 => {
            row.insert("tag".into(), json!(record.args[0].to_string()));
            row.insert(
                "marker_value".into(),
                json!(format!("0x{:x}", record.args[1])),
            );
        }
        12 => {
            row.insert("repeat_count".into(), json!(record.args[0].to_string()));
            row.insert(
                "original_kind".into(),
                json!(kind_name(record.args[1] as u32)),
            );
        }
        13 => {
            row.insert("frame".into(), json!(record.args[7].to_string()));
            row.insert("reg_id".into(), json!(record.args[0].to_string()));
            row.insert("width".into(), json!(record.args[3].to_string()));
            row.insert("part".into(), json!(record.args[4].to_string()));
            row.insert(
                "value".into(),
                json!(format!("0x{:016x}{:016x}", record.args[2], record.args[1])),
            );
        }
        _ => {
            row.insert(
                "args".into(),
                json!(record
                    .args
                    .iter()
                    .map(|value| format!("0x{value:x}"))
                    .collect::<Vec<_>>()),
            );
        }
    }
}

fn parse_fields(args: &Map<String, Value>) -> Result<Vec<String>, HubError> {
    let Some(values) = args.get("fields").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    if values.len() > 32 {
        return Err(HubError::Validation(
            "Trace field projection exceeds 32 entries".into(),
        ));
    }
    const ALLOWED: &[&str] = &[
        "sequence",
        "kind",
        "kind_id",
        "thread_id",
        "address",
        "size",
        "bytes",
        "memory",
        "access",
        "value",
        "target",
        "taken",
        "number",
        "phase",
        "arguments",
        "return_value",
        "errno",
        "reason",
        "info",
        "context_ip",
        "tag",
        "marker_value",
        "frame",
        "reg_id",
        "width",
        "part",
        "repeat_count",
        "original_kind",
        "args",
    ];
    values
        .iter()
        .map(|value| {
            let field = value.as_str().ok_or_else(|| {
                HubError::Validation("Trace projected field must be a string".into())
            })?;
            if !ALLOWED.contains(&field) {
                return Err(HubError::Validation(format!(
                    "unsupported Trace projected field: {field}"
                )));
            }
            Ok(field.to_string())
        })
        .collect()
}

fn parse_kind(value: &str) -> Result<u32, HubError> {
    let lower = value.trim().to_ascii_lowercase();
    let kind = match lower.as_str() {
        "memory_plain" => 2,
        "exec_plain" => 3,
        "branch" | "branch_edge" => 4,
        "syscall" => 5,
        "exception" | "context_change" => 6,
        "exec" | "exec_bytes" => 9,
        "memory" | "mem_value" => 10,
        "marker" => 11,
        "repeat" => 12,
        "registers" | "reg_snapshot" => 13,
        _ => {
            return u32::try_from(parse_integer(value)?)
                .map_err(|_| HubError::Validation("Trace kind id is out of range".into()))
        }
    };
    Ok(kind)
}

fn kind_name(kind: u32) -> String {
    match kind {
        1 => "hook_regs".into(),
        2 => "memory_plain".into(),
        3 => "exec_plain".into(),
        4 => "branch".into(),
        5 => "syscall".into(),
        6 => "exception".into(),
        7 => "module_load".into(),
        8 => "module_unload".into(),
        9 => "exec".into(),
        10 => "memory".into(),
        11 => "marker".into(),
        12 => "repeat".into(),
        13 => "registers".into(),
        _ => format!("unknown_{kind}"),
    }
}

fn csv(events: &[Value]) -> String {
    const FIELDS: &[&str] = &[
        "sequence",
        "kind",
        "thread_id",
        "address",
        "memory",
        "access",
        "size",
        "value",
        "target",
        "taken",
        "number",
        "phase",
        "bytes",
    ];
    let mut data = FIELDS.join(",");
    data.push_str("\r\n");
    for event in events {
        data.push_str(
            &FIELDS
                .iter()
                .map(|field| {
                    let value = match event.get(field) {
                        None | Some(Value::Null) => String::new(),
                        Some(Value::String(value)) => value.clone(),
                        Some(value) => value.to_string(),
                    };
                    csv_cell(&value)
                })
                .collect::<Vec<_>>()
                .join(","),
        );
        data.push_str("\r\n");
    }
    data
}

fn csv_cell(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\r' | '\n'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn safe_filename(requested: &str, extension: &str) -> Result<String, HubError> {
    let requested = requested.trim();
    let suffix = format!(".{extension}");
    let stem = requested.strip_suffix(&suffix).unwrap_or(requested);
    if stem.is_empty()
        || stem.len() > 128
        || !stem.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(HubError::Validation(
            "Trace export filename must use 1..128 ASCII letters, digits, dot, dash, or underscore"
                .into(),
        ));
    }
    Ok(format!("{stem}.{extension}"))
}

fn required_string<'a>(args: &'a Map<String, Value>, name: &str) -> Result<&'a str, HubError> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| HubError::Validation(format!("{name} must be string")))
}

fn parse_integer(value: &str) -> Result<u64, HubError> {
    let value = value.trim();
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16)
    } else {
        value.parse()
    }
    .map_err(|_| HubError::Validation(format!("invalid Trace integer: {value}")))
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("four-byte field"))
}

fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("eight-byte field"))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_record(
        bytes: &mut Vec<u8>,
        sequence: u64,
        kind: u32,
        thread_id: u32,
        address: u64,
        args: [u64; 8],
    ) {
        bytes.extend_from_slice(&sequence.to_le_bytes());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&thread_id.to_le_bytes());
        bytes.extend_from_slice(&address.to_le_bytes());
        for value in args {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn fixture() -> String {
        let metadata = br#"{"target":"fixture.exe","kinds":[9,10]}"#;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"PBTR");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(metadata);
        push_record(
            &mut bytes,
            1,
            9,
            7,
            0x140001000,
            [1, 0x90, 0, 0, 0, 0, 0, 0],
        );
        push_record(
            &mut bytes,
            2,
            10,
            7,
            0x140001001,
            [0x2000, 4, 0, 0x11223344, 0, 0, 0, 0],
        );
        push_record(
            &mut bytes,
            3,
            10,
            8,
            0x140001002,
            [0x2000, 4, 1, 0x55667788, 0, 0, 0, 0],
        );
        let path = std::env::temp_dir().join(format!(
            "pinbridge-trace-query-test-{}-{}.pbtr",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn large_fixture(records: u64) -> String {
        let metadata = br#"{"target":"large-fixture.exe","kinds":[9,10]}"#;
        let mut bytes =
            Vec::with_capacity(HEADER_LEN + metadata.len() + records as usize * RECORD_LEN);
        bytes.extend_from_slice(b"PBTR");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(metadata);
        for sequence in 1..=records {
            let memory = 0x2000 + (sequence % 32) * 8;
            let kind = if sequence % 2 == 0 { 9 } else { 10 };
            push_record(
                &mut bytes,
                sequence,
                kind,
                7 + (sequence % 4) as u32,
                0x140001000 + sequence % 256,
                [memory, 8, sequence & 1, sequence, 0, 0, 0, 0],
            );
        }
        let path = std::env::temp_dir().join(format!(
            "pinbridge-trace-query-large-{}-{}.pbtr",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn cleanup(path: &str) {
        let _ = std::fs::remove_file(sidecar_path(Path::new(path)));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn exact_kind_index_is_bounded_and_payload_is_opt_in() {
        let path = fixture();
        let args = [
            ("index".into(), json!("kind")),
            ("key".into(), json!("memory")),
            ("limit".into(), json!("1")),
        ]
        .into_iter()
        .collect();
        let result = query(&path, &args).unwrap();
        assert_eq!(result["matched_total"], "2");
        assert_eq!(result["returned"], "1");
        assert_eq!(result["events"][0]["sequence"], "3");
        assert!(result["events"][0].get("memory").is_none());
        assert_eq!(result["next_before"], "3");

        let mut payload = args;
        payload.insert("payload".into(), json!(true));
        payload.insert("before".into(), json!("3"));
        let result = query(&path, &payload).unwrap();
        assert_eq!(result["events"][0]["sequence"], "2");
        assert_eq!(result["events"][0]["memory"], "0x2000");
        assert_eq!(result["events"][0]["access"], "read");
        cleanup(&path);
    }

    #[test]
    fn exact_memory_index_exports_only_the_requested_page() {
        let path = fixture();
        let args = [
            ("index".into(), json!("memory")),
            ("key".into(), json!("0x2000")),
            ("limit".into(), json!("2")),
            ("payload".into(), json!(true)),
            ("format".into(), json!("jsonl")),
            ("delivery".into(), json!("inline")),
        ]
        .into_iter()
        .collect();
        let result = export(&path, &args).unwrap();
        assert_eq!(result["rows"], "2");
        assert!(result["data"]
            .as_str()
            .is_some_and(|data| data.contains("0x2000")));
        cleanup(&path);
    }

    #[test]
    fn large_trace_builds_and_reuses_local_database() {
        let path = large_fixture(160_000);
        let args = [
            ("index".into(), json!("kind")),
            ("key".into(), json!("exec")),
            ("limit".into(), json!("50")),
            ("payload".into(), json!(true)),
        ]
        .into_iter()
        .collect();
        let result = query(&path, &args).unwrap();
        assert_eq!(result["query_store"], "sqlite");
        assert_eq!(result["physical_records"], "160000");
        assert_eq!(result["matched_total"], "80000");
        assert_eq!(result["returned"], "50");
        let database = sidecar_path(Path::new(&path));
        assert!(database.is_file());

        let second = query(&path, &args).unwrap();
        assert_eq!(second["matched_total"], "80000");
        cleanup(&path);
    }
}

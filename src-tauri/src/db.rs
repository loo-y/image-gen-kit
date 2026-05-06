use crate::app_paths;
use crate::sqlite::Connection;
use crate::types::{
    Generation, GenerationDetail, GenerationInputImage, GenerationOutput, ProviderProfile,
    StoredProviderProfile,
};

pub fn init_database() -> Result<(), String> {
    let db = open()?;
    db.execute(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS provider_profiles (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            provider_type TEXT NOT NULL,
            base_url TEXT NOT NULL,
            model_default TEXT NOT NULL,
            network_timeout_minutes INTEGER NOT NULL DEFAULT 15,
            api_key_ref TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS generations (
            id TEXT PRIMARY KEY,
            prompt TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            provider_type TEXT NOT NULL,
            provider_name TEXT NOT NULL,
            model TEXT NOT NULL,
            status TEXT NOT NULL,
            size TEXT NOT NULL,
            quality TEXT NOT NULL,
            output_format TEXT NOT NULL,
            params_json TEXT NOT NULL,
            response_json TEXT,
            error_message TEXT,
            revised_prompt TEXT,
            created_at INTEGER NOT NULL,
            completed_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS generation_outputs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            generation_id TEXT NOT NULL,
            path TEXT NOT NULL,
            format TEXT NOT NULL,
            width INTEGER,
            height INTEGER,
            file_size INTEGER NOT NULL,
            output_index INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY(generation_id) REFERENCES generations(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS generation_input_images (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            generation_id TEXT NOT NULL,
            path TEXT NOT NULL,
            name TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            input_index INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY(generation_id) REFERENCES generations(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_generations_created_at ON generations(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_outputs_generation_id ON generation_outputs(generation_id);
        CREATE INDEX IF NOT EXISTS idx_input_images_generation_id ON generation_input_images(generation_id);
        "#,
    )?;
    ensure_column(
        &db,
        "provider_profiles",
        "network_timeout_minutes",
        "INTEGER NOT NULL DEFAULT 15",
    )?;
    ensure_column(&db, "generations", "response_json", "TEXT")?;
    ensure_default_profile(&db)
}

pub fn open() -> Result<Connection, String> {
    let path = app_paths::database_path()?;
    let db = Connection::open(&path)?;
    db.execute(
        r#"
        PRAGMA busy_timeout = 10000;
        PRAGMA foreign_keys = ON;
        "#,
    )?;
    Ok(db)
}

pub fn list_profiles() -> Result<Vec<ProviderProfile>, String> {
    init_database()?;
    let db = open()?;
    let mut stmt = db.prepare(
        r#"
        SELECT id, name, provider_type, base_url, model_default, network_timeout_minutes,
               api_key_ref, created_at, updated_at
        FROM provider_profiles
        ORDER BY created_at ASC
        "#,
    )?;
    let mut profiles = Vec::new();
    while stmt.step()? {
        profiles.push(profile_from_stmt(&stmt).profile);
    }
    Ok(profiles)
}

pub fn get_profile(id: &str) -> Result<Option<StoredProviderProfile>, String> {
    init_database()?;
    let db = open()?;
    let mut stmt = db.prepare(
        r#"
        SELECT id, name, provider_type, base_url, model_default, network_timeout_minutes,
               api_key_ref, created_at, updated_at
        FROM provider_profiles
        WHERE id = ?1
        "#,
    )?;
    stmt.bind_text(1, id)?;
    if stmt.step()? {
        Ok(Some(profile_from_stmt(&stmt)))
    } else {
        Ok(None)
    }
}

pub fn first_profile() -> Result<StoredProviderProfile, String> {
    init_database()?;
    let db = open()?;
    let mut stmt = db.prepare(
        r#"
        SELECT id, name, provider_type, base_url, model_default, network_timeout_minutes,
               api_key_ref, created_at, updated_at
        FROM provider_profiles
        ORDER BY created_at ASC
        LIMIT 1
        "#,
    )?;
    if stmt.step()? {
        Ok(profile_from_stmt(&stmt))
    } else {
        Err("No provider profile exists".to_string())
    }
}

pub fn upsert_profile(
    id: &str,
    name: &str,
    provider_type: &str,
    base_url: &str,
    model_default: &str,
    network_timeout_minutes: i64,
    api_key_ref: Option<&str>,
    now: i64,
) -> Result<ProviderProfile, String> {
    init_database()?;
    let db = open()?;
    let existing_created_at = get_profile(id)?
        .map(|profile| profile.profile.created_at)
        .unwrap_or(now);
    let mut stmt = db.prepare(
        r#"
        INSERT INTO provider_profiles (
            id, name, provider_type, base_url, model_default, network_timeout_minutes,
            api_key_ref, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            provider_type = excluded.provider_type,
            base_url = excluded.base_url,
            model_default = excluded.model_default,
            network_timeout_minutes = excluded.network_timeout_minutes,
            api_key_ref = excluded.api_key_ref,
            updated_at = excluded.updated_at
        "#,
    )?;
    stmt.bind_text(1, id)?;
    stmt.bind_text(2, name)?;
    stmt.bind_text(3, provider_type)?;
    stmt.bind_text(4, base_url)?;
    stmt.bind_text(5, model_default)?;
    stmt.bind_i64(6, network_timeout_minutes)?;
    stmt.bind_optional_text(7, api_key_ref)?;
    stmt.bind_i64(8, existing_created_at)?;
    stmt.bind_i64(9, now)?;
    stmt.step()?;
    get_profile(id)?
        .map(|stored| stored.profile)
        .ok_or_else(|| "Profile was not saved".to_string())
}

pub fn insert_generation(generation: &Generation) -> Result<(), String> {
    init_database()?;
    let db = open()?;
    let mut stmt = db.prepare(
        r#"
        INSERT INTO generations (
            id, prompt, provider_id, provider_type, provider_name, model, status,
            size, quality, output_format, params_json, response_json, error_message, revised_prompt,
            created_at, completed_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        "#,
    )?;
    stmt.bind_text(1, &generation.id)?;
    stmt.bind_text(2, &generation.prompt)?;
    stmt.bind_text(3, &generation.provider_id)?;
    stmt.bind_text(4, &generation.provider_type)?;
    stmt.bind_text(5, &generation.provider_name)?;
    stmt.bind_text(6, &generation.model)?;
    stmt.bind_text(7, &generation.status)?;
    stmt.bind_text(8, &generation.size)?;
    stmt.bind_text(9, &generation.quality)?;
    stmt.bind_text(10, &generation.output_format)?;
    stmt.bind_text(11, &generation.params_json)?;
    stmt.bind_optional_text(12, generation.response_json.as_deref())?;
    stmt.bind_optional_text(13, generation.error_message.as_deref())?;
    stmt.bind_optional_text(14, generation.revised_prompt.as_deref())?;
    stmt.bind_i64(15, generation.created_at)?;
    match generation.completed_at {
        Some(value) => stmt.bind_i64(16, value)?,
        None => stmt.bind_null(16)?,
    }
    stmt.step()?;
    Ok(())
}

pub fn update_generation_success(
    id: &str,
    revised_prompt: Option<&str>,
    response_json: Option<&str>,
    completed_at: i64,
) -> Result<(), String> {
    let db = open()?;
    let mut stmt = db.prepare(
        r#"
        UPDATE generations
        SET status = 'succeeded', error_message = NULL, revised_prompt = ?2,
            response_json = ?3, completed_at = ?4
        WHERE id = ?1
        "#,
    )?;
    stmt.bind_text(1, id)?;
    stmt.bind_optional_text(2, revised_prompt)?;
    stmt.bind_optional_text(3, response_json)?;
    stmt.bind_i64(4, completed_at)?;
    stmt.step()?;
    Ok(())
}

pub fn update_generation_failed(
    id: &str,
    message: &str,
    response_json: Option<&str>,
    completed_at: i64,
) -> Result<(), String> {
    let db = open()?;
    let mut stmt = db.prepare(
        r#"
        UPDATE generations
        SET status = 'failed', error_message = ?2, response_json = ?3, completed_at = ?4
        WHERE id = ?1
        "#,
    )?;
    stmt.bind_text(1, id)?;
    stmt.bind_text(2, message)?;
    stmt.bind_optional_text(3, response_json)?;
    stmt.bind_i64(4, completed_at)?;
    stmt.step()?;
    Ok(())
}

pub fn insert_output(output: &GenerationOutput) -> Result<(), String> {
    let db = open()?;
    let mut stmt = db.prepare(
        r#"
        INSERT INTO generation_outputs (
            generation_id, path, format, width, height, file_size, output_index, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
    )?;
    stmt.bind_text(1, &output.generation_id)?;
    stmt.bind_text(2, &output.path)?;
    stmt.bind_text(3, &output.format)?;
    bind_optional_i64(&mut stmt, 4, output.width)?;
    bind_optional_i64(&mut stmt, 5, output.height)?;
    stmt.bind_i64(6, output.file_size)?;
    stmt.bind_i64(7, output.output_index)?;
    stmt.bind_i64(8, output.created_at)?;
    stmt.step()?;
    Ok(())
}

pub fn insert_input_image(input: &GenerationInputImage) -> Result<(), String> {
    let db = open()?;
    let mut stmt = db.prepare(
        r#"
        INSERT INTO generation_input_images (
            generation_id, path, name, mime_type, file_size, input_index, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )?;
    stmt.bind_text(1, &input.generation_id)?;
    stmt.bind_text(2, &input.path)?;
    stmt.bind_text(3, &input.name)?;
    stmt.bind_text(4, &input.mime_type)?;
    stmt.bind_i64(5, input.file_size)?;
    stmt.bind_i64(6, input.input_index)?;
    stmt.bind_i64(7, input.created_at)?;
    stmt.step()?;
    Ok(())
}

pub fn list_generation_details(
    query: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<GenerationDetail>, String> {
    init_database()?;
    let db = open()?;
    let mut generations = Vec::new();
    if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
        let pattern = format!("%{}%", query.trim());
        let mut stmt = db.prepare(
            r#"
            SELECT id, prompt, provider_id, provider_type, provider_name, model, status,
                   size, quality, output_format, params_json, response_json, error_message, revised_prompt,
                   created_at, completed_at
            FROM generations
            WHERE prompt LIKE ?1 OR model LIKE ?1 OR provider_name LIKE ?1
            ORDER BY created_at DESC, id DESC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;
        stmt.bind_text(1, &pattern)?;
        stmt.bind_i64(2, limit)?;
        stmt.bind_i64(3, offset)?;
        while stmt.step()? {
            generations.push(generation_from_stmt(&stmt));
        }
    } else {
        let mut stmt = db.prepare(
            r#"
            SELECT id, prompt, provider_id, provider_type, provider_name, model, status,
                   size, quality, output_format, params_json, response_json, error_message, revised_prompt,
                   created_at, completed_at
            FROM generations
            ORDER BY created_at DESC, id DESC
            LIMIT ?1 OFFSET ?2
            "#,
        )?;
        stmt.bind_i64(1, limit)?;
        stmt.bind_i64(2, offset)?;
        while stmt.step()? {
            generations.push(generation_from_stmt(&stmt));
        }
    }

    generations
        .into_iter()
        .map(|generation| {
            let outputs = list_outputs(&generation.id)?;
            let input_images = list_input_images(&generation.id)?;
            Ok(GenerationDetail {
                generation,
                outputs,
                input_images,
            })
        })
        .collect()
}

pub fn get_generation_detail(id: &str) -> Result<Option<GenerationDetail>, String> {
    init_database()?;
    let db = open()?;
    let mut stmt = db.prepare(
        r#"
        SELECT id, prompt, provider_id, provider_type, provider_name, model, status,
               size, quality, output_format, params_json, response_json, error_message, revised_prompt,
               created_at, completed_at
        FROM generations
        WHERE id = ?1
        "#,
    )?;
    stmt.bind_text(1, id)?;
    if stmt.step()? {
        let generation = generation_from_stmt(&stmt);
        let outputs = list_outputs(&generation.id)?;
        let input_images = list_input_images(&generation.id)?;
        Ok(Some(GenerationDetail {
            generation,
            outputs,
            input_images,
        }))
    } else {
        Ok(None)
    }
}

pub fn list_outputs(generation_id: &str) -> Result<Vec<GenerationOutput>, String> {
    let db = open()?;
    let mut stmt = db.prepare(
        r#"
        SELECT id, generation_id, path, format, width, height, file_size, output_index, created_at
        FROM generation_outputs
        WHERE generation_id = ?1
        ORDER BY output_index ASC
        "#,
    )?;
    stmt.bind_text(1, generation_id)?;
    let mut outputs = Vec::new();
    while stmt.step()? {
        outputs.push(output_from_stmt(&stmt));
    }
    Ok(outputs)
}

pub fn list_input_images(generation_id: &str) -> Result<Vec<GenerationInputImage>, String> {
    let db = open()?;
    let mut stmt = db.prepare(
        r#"
        SELECT id, generation_id, path, name, mime_type, file_size, input_index, created_at
        FROM generation_input_images
        WHERE generation_id = ?1
        ORDER BY input_index ASC
        "#,
    )?;
    stmt.bind_text(1, generation_id)?;
    let mut inputs = Vec::new();
    while stmt.step()? {
        inputs.push(input_image_from_stmt(&stmt));
    }
    Ok(inputs)
}

pub fn delete_generation(id: &str) -> Result<Vec<String>, String> {
    init_database()?;
    let outputs = list_outputs(id)?;
    let inputs = list_input_images(id)?;
    let paths = outputs
        .into_iter()
        .map(|output| output.path)
        .chain(inputs.into_iter().map(|input| input.path))
        .collect();
    let db = open()?;
    let mut stmt = db.prepare("DELETE FROM generations WHERE id = ?1")?;
    stmt.bind_text(1, id)?;
    stmt.step()?;
    Ok(paths)
}

fn ensure_default_profile(db: &Connection) -> Result<(), String> {
    let now = crate::commands::now_millis();
    let mut stmt = db.prepare(
        r#"
        INSERT OR IGNORE INTO provider_profiles (
            id, name, provider_type, base_url, model_default, network_timeout_minutes,
            api_key_ref, created_at, updated_at
        )
        VALUES ('openai-default', 'OpenAI', 'openai', 'https://api.openai.com/v1', 'gpt-image-2', 15, NULL, ?1, ?1)
        "#,
    )?;
    stmt.bind_i64(1, now)?;
    stmt.step()?;
    Ok(())
}

fn ensure_column(
    db: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
    match db.execute(&sql) {
        Ok(()) => Ok(()),
        Err(err) if err.to_lowercase().contains("duplicate column name") => Ok(()),
        Err(err) => Err(err),
    }
}

fn profile_from_stmt(stmt: &crate::sqlite::Statement<'_>) -> StoredProviderProfile {
    let api_key_ref = stmt.column_text(6).filter(|value| !value.is_empty());
    StoredProviderProfile {
        profile: ProviderProfile {
            id: stmt.column_text(0).unwrap_or_default(),
            name: stmt.column_text(1).unwrap_or_default(),
            provider_type: stmt.column_text(2).unwrap_or_default(),
            base_url: stmt.column_text(3).unwrap_or_default(),
            model_default: stmt.column_text(4).unwrap_or_default(),
            network_timeout_minutes: stmt.column_i64(5).clamp(1, 120),
            api_key_saved: api_key_ref.is_some(),
            created_at: stmt.column_i64(7),
            updated_at: stmt.column_i64(8),
        },
        api_key_ref,
    }
}

fn generation_from_stmt(stmt: &crate::sqlite::Statement<'_>) -> Generation {
    Generation {
        id: stmt.column_text(0).unwrap_or_default(),
        prompt: stmt.column_text(1).unwrap_or_default(),
        provider_id: stmt.column_text(2).unwrap_or_default(),
        provider_type: stmt.column_text(3).unwrap_or_default(),
        provider_name: stmt.column_text(4).unwrap_or_default(),
        model: stmt.column_text(5).unwrap_or_default(),
        status: stmt.column_text(6).unwrap_or_default(),
        size: stmt.column_text(7).unwrap_or_default(),
        quality: stmt.column_text(8).unwrap_or_default(),
        output_format: stmt.column_text(9).unwrap_or_default(),
        params_json: stmt.column_text(10).unwrap_or_default(),
        response_json: stmt.column_text(11).filter(|value| !value.is_empty()),
        error_message: stmt.column_text(12).filter(|value| !value.is_empty()),
        revised_prompt: stmt.column_text(13).filter(|value| !value.is_empty()),
        created_at: stmt.column_i64(14),
        completed_at: optional_i64(stmt, 15),
    }
}

fn output_from_stmt(stmt: &crate::sqlite::Statement<'_>) -> GenerationOutput {
    GenerationOutput {
        id: stmt.column_i64(0),
        generation_id: stmt.column_text(1).unwrap_or_default(),
        path: stmt.column_text(2).unwrap_or_default(),
        format: stmt.column_text(3).unwrap_or_default(),
        width: optional_i64(stmt, 4),
        height: optional_i64(stmt, 5),
        file_size: stmt.column_i64(6),
        output_index: stmt.column_i64(7),
        created_at: stmt.column_i64(8),
    }
}

fn input_image_from_stmt(stmt: &crate::sqlite::Statement<'_>) -> GenerationInputImage {
    GenerationInputImage {
        id: stmt.column_i64(0),
        generation_id: stmt.column_text(1).unwrap_or_default(),
        path: stmt.column_text(2).unwrap_or_default(),
        name: stmt.column_text(3).unwrap_or_default(),
        mime_type: stmt.column_text(4).unwrap_or_default(),
        file_size: stmt.column_i64(5),
        input_index: stmt.column_i64(6),
        created_at: stmt.column_i64(7),
    }
}

fn bind_optional_i64(
    stmt: &mut crate::sqlite::Statement<'_>,
    index: i32,
    value: Option<i64>,
) -> Result<(), String> {
    match value {
        Some(value) => stmt.bind_i64(index, value),
        None => stmt.bind_null(index),
    }
}

fn optional_i64(stmt: &crate::sqlite::Statement<'_>, index: i32) -> Option<i64> {
    if stmt.column_text(index).is_none() {
        None
    } else {
        Some(stmt.column_i64(index))
    }
}

-- Initial schema for EvePI

CREATE TABLE IF NOT EXISTS accounts (
    account_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    label        TEXT NOT NULL UNIQUE,
    created_at   TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
);

CREATE TABLE IF NOT EXISTS characters (
    character_id      INTEGER PRIMARY KEY,  -- EVE character ID
    account_id        INTEGER REFERENCES accounts(account_id),
    character_name    TEXT,
    current_system_id INTEGER,              -- updated on sync; NULL if unknown
    last_synced_at    TEXT
);

CREATE TABLE IF NOT EXISTS solar_systems (
    solar_system_id   INTEGER PRIMARY KEY,
    solar_system_name TEXT NOT NULL,
    cached_at         TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
);

CREATE TABLE IF NOT EXISTS colonies (
    character_id    INTEGER NOT NULL REFERENCES characters(character_id),
    planet_id       INTEGER NOT NULL,
    planet_type     TEXT NOT NULL,
    planet_index    INTEGER NOT NULL DEFAULT 1,  -- 1-based index within system (drives "Gas III" label)
    solar_system_id INTEGER NOT NULL,
    upgrade_level   INTEGER NOT NULL DEFAULT 0,
    num_pins        INTEGER NOT NULL DEFAULT 0,
    last_update     TEXT,
    last_synced_at  TEXT,
    PRIMARY KEY (character_id, planet_id)
);

CREATE TABLE IF NOT EXISTS pins (
    pin_id         INTEGER NOT NULL,
    character_id   INTEGER NOT NULL REFERENCES characters(character_id),
    planet_id      INTEGER NOT NULL,
    type_id        INTEGER NOT NULL,
    is_extractor   INTEGER NOT NULL DEFAULT 0,  -- boolean
    schematic_id   INTEGER,
    expiry_time    TEXT,
    install_time   TEXT,
    PRIMARY KEY (pin_id, character_id, planet_id)
);

CREATE TABLE IF NOT EXISTS extractor_heads (
    pin_id     INTEGER NOT NULL,
    head_id    INTEGER NOT NULL,
    latitude   REAL NOT NULL,
    longitude  REAL NOT NULL,
    PRIMARY KEY (pin_id, head_id)
);

CREATE TABLE IF NOT EXISTS routes (
    route_id            INTEGER NOT NULL,
    character_id        INTEGER NOT NULL REFERENCES characters(character_id),
    planet_id           INTEGER NOT NULL,
    source_pin_id       INTEGER NOT NULL,
    destination_pin_id  INTEGER NOT NULL,
    content_type_id     INTEGER NOT NULL,
    quantity            REAL NOT NULL,
    PRIMARY KEY (route_id, character_id, planet_id)
);

CREATE TABLE IF NOT EXISTS customs_offices (
    planet_id   INTEGER PRIMARY KEY,
    tax_rate    REAL NOT NULL DEFAULT 0.0,  -- 0.0–1.0; defaults to 0%
    from_esi    INTEGER NOT NULL DEFAULT 0, -- boolean: 1 if pulled from ESI
    updated_at  TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
);

CREATE TABLE IF NOT EXISTS schematics (
    schematic_id    INTEGER PRIMARY KEY,
    schematic_name  TEXT NOT NULL,
    cycle_time      INTEGER NOT NULL,
    cached_at       TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
);

CREATE TABLE IF NOT EXISTS schematic_inputs (
    schematic_id  INTEGER NOT NULL REFERENCES schematics(schematic_id),
    type_id       INTEGER NOT NULL,
    quantity      INTEGER NOT NULL,
    PRIMARY KEY (schematic_id, type_id)
);

CREATE TABLE IF NOT EXISTS schematic_output (
    schematic_id  INTEGER NOT NULL PRIMARY KEY REFERENCES schematics(schematic_id),
    type_id       INTEGER NOT NULL,
    quantity      INTEGER NOT NULL
);

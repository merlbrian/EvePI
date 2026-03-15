-- Cache of EVE type names (items, resources, buildings).
-- Populated on-demand during sync; no DATABASE_URL needed at compile time.
CREATE TABLE IF NOT EXISTS eve_types (
    type_id   INTEGER PRIMARY KEY,
    type_name TEXT NOT NULL,
    cached_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
);

-- Store which P0/P1 resource an extractor pin is currently mining.
-- NULL for non-extractor pins.
ALTER TABLE pins ADD COLUMN product_type_id INTEGER;

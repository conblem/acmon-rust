CREATE TABLE account (
    id SERIAL PRIMARY KEY,
    -- valid emails cannot be longer than 320 characters
    email VARCHAR(320) UNIQUE
);

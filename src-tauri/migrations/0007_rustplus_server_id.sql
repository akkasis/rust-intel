-- Новые спаривания Rust+ приходят с GUID сервера Facepunch вместо ip/port.
ALTER TABLE rustplus_servers ADD COLUMN server_id TEXT;

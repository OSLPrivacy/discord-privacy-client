SELECT hex(id) AS id, length(data) AS data_bytes, size_bytes, expires_at FROM blobs LIMIT 5;

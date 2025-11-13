AOF (Append only File)

- Supports appending variable length records to a logical chunked File
- Chunks are between 128kb to 4GB
- Truncate Front is when full Chunks are deleted from the logical front of the File
- Truncate Back is when a logical size needs to be set deleting any full chunks beyond that size
marker and promoting the Chunk it points to as the WAL chunk.
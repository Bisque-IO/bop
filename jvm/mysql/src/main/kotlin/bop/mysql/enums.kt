package bop.mysql

/**
 * The flags used in COM_STMT_EXECUTE.
 *
 * See also
 * Protocol_classic::parse_packet, mysql_int_serialize_param_data
 */
enum class CursorType {
   CURSOR_TYPE_NO_CURSOR,
   CURSOR_TYPE_READ_ONLY,
   CURSOR_TYPE_FOR_UPDATE,
   CURSOR_TYPE_SCROLLABLE,

   /**
    * On when the client will send the parameter count even for 0 parameters.
    */
   PARAMETER_COUNT_AVAILABLE,
}

/**
 * Column types for MySQL
 *
 * Note: Keep include/mysql/components/services/bits/stored_program_bits.h in sync with this.
 */
enum class FieldType {
   MYSQL_TYPE_DECIMAL,
   MYSQL_TYPE_TINY,
   MYSQL_TYPE_SHORT,
   MYSQL_TYPE_LONG,
   MYSQL_TYPE_FLOAT,
   MYSQL_TYPE_DOUBLE,
   MYSQL_TYPE_NULL,
   MYSQL_TYPE_TIMESTAMP,
   MYSQL_TYPE_LONGLONG,
   MYSQL_TYPE_INT24,
   MYSQL_TYPE_DATE,
   MYSQL_TYPE_TIME,
   MYSQL_TYPE_DATETIME,
   MYSQL_TYPE_YEAR,

   /**
    * Internal to MySQL.
    *
    * Not used in protocol
    */
   MYSQL_TYPE_NEWDATE,

   MYSQL_TYPE_VARCHAR,
   MYSQL_TYPE_BIT,
   MYSQL_TYPE_TIMESTAMP2,

   /**
    * Internal to MySQL.
    *
    * Not used in protocol
    */
   MYSQL_TYPE_DATETIME2,

   /**
    * Internal to MySQL.
    *
    * Not used in protocol
    */
   MYSQL_TYPE_TIME2,

   /**
    * Used for replication only.
    */
   MYSQL_TYPE_TYPED_ARRAY,

   MYSQL_TYPE_VECTOR,
   MYSQL_TYPE_INVALID,

   /**
    * Currently just a placeholder.
    */
   MYSQL_TYPE_BOOL,

   MYSQL_TYPE_JSON,
   MYSQL_TYPE_NEWDECIMAL,
   MYSQL_TYPE_ENUM,
   MYSQL_TYPE_SET,
   MYSQL_TYPE_TINY_BLOB,
   MYSQL_TYPE_MEDIUM_BLOB,
   MYSQL_TYPE_LONG_BLOB,
   MYSQL_TYPE_BLOB,
   MYSQL_TYPE_VAR_STRING,
   MYSQL_TYPE_STRING,
   MYSQL_TYPE_GEOMETRY;
}

/**
 * options for mysql_options()
 */
enum class MySQLSetOption {
   MYSQL_OPTION_MULTI_STATEMENTS_ON,
   MYSQL_OPTION_MULTI_STATEMENTS_OFF;
}

enum class ResultSetMetadata {
   /**
    * No metadata will be sent.
    */
   RESULTSET_METADATA_NONE,

   /**
    * The server will send all metadata.
    */
   RESULTSET_METADATA_FULL
}

/**
 * Type of state change information that the server can include in the Ok packet.
 *
 * Note
 * session_state_type shouldn't go past 255 (i.e. 1-byte boundary).
 * Modify the definition of SESSION_TRACK_END when a new member is added.
 */
enum class SessionState {
   /**
    * Session system variables.
    */
   SESSION_TRACK_SYSTEM_VARIABLES,

   /**
    * Current schema.
    */
   SESSION_TRACK_SCHEMA,

   /**
    * track session state changes
    */
   SESSION_TRACK_STATE_CHANGE,

   /**
    * See also: session_track_gtids.
    */
   SESSION_TRACK_GTIDS,

   /**
    * Transaction chistics.
    */
   SESSION_TRACK_TRANSACTION_CHARACTERISTICS,

   /**
    * Transaction state.
    */
   SESSION_TRACK_TRANSACTION_STATE
}

/**
 * The status flags are a bit-field.
 */
enum class ServerStatus {
   /**
    * Is raised when a multi-statement transaction has been started, either explicitly, by means
    * of BEGIN or COMMIT AND CHAIN, or implicitly, by the first transactional statement, when
    * autocommit=off.
    */
   SERVER_STATUS_IN_TRANS,

   /**
    * Server in auto_commit mode.
    */
   SERVER_STATUS_AUTOCOMMIT,

   /**
    * Multi query - next query exists.
    */
   SERVER_MORE_RESULTS_EXISTS,

   /**
    *
    */
   SERVER_QUERY_NO_GOOD_INDEX_USED,

   /**
    *
    */
   SERVER_QUERY_NO_INDEX_USED,

   /**
    * The server was able to fulfill the clients request and opened a read-only non-scrollable
    * cursor for a query.
    *
    * This flag comes in reply to COM_STMT_EXECUTE and COM_STMT_FETCH commands.
    * Used by Binary Protocol Resultset to signal that COM_STMT_FETCH must be
    * used to fetch the row-data.
    */
   SERVER_STATUS_CURSOR_EXISTS,

   /**
    * This flag is sent when a read-only cursor is exhausted, in reply to COM_STMT_FETCH command.
    */
   SERVER_STATUS_LAST_ROW_SENT,

   /**
    * A database was dropped.
    */
   SERVER_STATUS_DB_DROPPED,

   SERVER_STATUS_NO_BACKSLASH_ESCAPES,

   /**
    * Sent to the client if after a prepared statement reprepare we discovered that the new
    * statement returns a different number of result set columns.
    */
   SERVER_STATUS_METADATA_CHANGED,

   SERVER_QUERY_WAS_SLOW,

   /**
    * To mark ResultSet containing output parameter values.
    */
   SERVER_PS_OUT_PARAMS,

   /**
    * Set at the same time as SERVER_STATUS_IN_TRANS if the started multi-statement
    * transaction is a read-only transaction.
    *
    * Cleared when the transaction commits or aborts. Since this flag is sent to clients
    * in OK and EOF packets, the flag indicates the transaction status at the end of
    * command execution.
    */
   SERVER_STATUS_IN_TRANS_READONLY,

   /**
    * This status flag, when on, implies that one of the state information has changed on the
    * server because of the execution of the last statement.
    */
   SERVER_SESSION_STATE_CHANGED;

   fun has(value: Int): Boolean = (value and (1 shl ordinal)) != 0

   fun set(value: Int): Int = value or (1 shl ordinal)

   fun unset(value: Int): Int = value and (1 shl ordinal).inv()

   companion object {
      @JvmStatic
      fun has(value: Int, flag: ServerStatus): Boolean = (value and (1 shl flag.ordinal)) != 0

      @JvmStatic
      fun set(value: Int, flag: ServerStatus): Int = value or (1 shl flag.ordinal)

      @JvmStatic
      fun unset(value: Int, flag: ServerStatus): Int = value and (1 shl flag.ordinal).inv()
   }
}

/**
 * Values for the flags bitmask used by Send_field:flags.
 *
 * Currently need to fit into 32 bits.
 *
 * Each bit represents an optional feature of the protocol.
 *
 * Both the client and the server are sending these.
 *
 * The intersection of the two determines what optional parts of the protocol will be used.
 */
object ColumnDefFlags {
   /**
    * Field can't be NULL.
    */
   const val NOT_NULL: Int = 1

   /**
    * Field is part of a primary key.
    */
   const val PRI_KEY: Int = 2

   /**
    * Field is part of a unique key.
    */
   const val UNIQUE_KEY: Int = 4

   /**
    * Field is part of a key.
    */
   const val MULTIPLE_KEY: Int = 8

   /**
    * Field is a blob.
    */
   const val BLOB: Int = 16

   /**
    * Field is unsigned.
    */
   const val UNSIGNED: Int = 32

   /**
    * Field is zerofill.
    */
   const val ZEROFILL: Int = 64

   /**
    * Field is binary
    */
   const val BINARY: Int = 128

   /**
    * field is an enum
    */
   const val ENUM: Int = 256

   /**
    * field is a autoincrement field
    */
   const val AUTO_INCREMENT: Int = 512

   /**
    * Field is a timestamp.
    */
   const val TIMESTAMP: Int = 1024

   /**
    * field is a set
    */
   const val SET: Int = 2048

   /**
    * Field doesn't have default value.
    */
   const val NO_DEFAULT_VALUE: Int = 4096

   /**
    * Field is set to NOW on UPDATE.
    */
   const val ON_UPDATE_NOW: Int = 8192

   /**
    * Intern; Part of some key.
    */
   const val PART_KEY: Int = 16384

   /**
    * Field is num (for clients)
    */
   const val NUM: Int = 32768

   /**
    * Intern: Used by sql_yacc.
    */
   const val UNIQUE: Int = 65536

   /**
    * Intern: Used by sql_yacc.
    */
   const val BINCMP: Int = 131072

   /**
    * Used to get fields in item tree .
    */
   const val GET_FIXED_FIELDS: Int = 1 shl 18

   /**
    * Field part of partition func.
    */
   const val FIELD_IN_PART_FUNC: Int = 1 shl 19

   /**
    * Intern: Field in TABLE object for new version of altered table,
    * which participates in a newly added index.
    */
   const val FIELD_IN_ADD_INDEX: Int = 1 shl 20

   /**
    * Intern: Field is being renamed.
    */
   const val FIELD_IS_RENAMED: Int = 1 shl 21
   const val FIELD_FLAGS_STORAGE_MEDIA: Int = 1 shl 22
   const val FIELD_FLAGS_STORAGE_MEDIA_MASK: Int = 1 shl 23

   /**
    * Field column format, bit 24-25.
    */
   const val FIELD_FLAGS_COLUMN_FORMAT: Int = 1 shl 24

   const val FIELD_FLAGS_COLUMN_FORMAT_MASK: Int = 1 shl 25

   /**
    * Intern: Field is being dropped.
    */
   const val FIELD_IS_DROPPED: Int = 1 shl 26

   /**
    * Field is explicitly specified as \ NULL by the user.
    */
   const val EXPLICIT_NULL: Int = 1 shl 27

   /**
    * Intern: Group field.
    *
    * Transient use in \ create_tmp_table
    */
   const val GROUP: Int = 1 shl 28

   /**
    * Field will not be loaded in secondary engine.
    */
   const val NOT_SECONDARY: Int = 1 shl 29

   /**
    * Field is explicitly marked as invisible by the user.
    */
   const val FIELD_IS_INVISIBLE: Int = 1 shl 30
}

object ClientCapabilities {
   /**
    * Use the improved version of Old Password Authentication.
    *
    * Not used.
    *
    * Note
    * Assumed to be set since 4.1.1.
    */
   const val CLIENT_LONG_PASSWORD: Int = 1

   /**
    * Send found rows instead of affected rows in EOF_Packet.
    */
   const val CLIENT_FOUND_ROWS: Int = 2

   /**
    * Get all column flags.
    *
    * Longer flags in Protocol::ColumnDefinition320.
    *
    * Server
    * Supports longer flags.
    *
    * Client
    * Expects longer flags.
    */
   const val CLIENT_LONG_FLAG: Int = 4

   /**
    * Database (schema) name can be specified on connect in Handshake Response Packet.
    */
   const val CLIENT_CONNECT_WITH_DB: Int = 8

   /**
    * DEPRECATED: Don't allow database.table.column.
    */
   const val CLIENT_NO_SCHEMA: Int = 16

   /**
    * Compression protocol supported.
    *
    * Server
    * Supports compression.
    *
    * Client
    * Switches to Compression compressed protocol after successful authentication.
    */
   const val CLIENT_COMPRESS: Int = 32

   /**
    * Special handling of ODBC behavior.
    */
   const val CLIENT_ODBC: Int = 64

   /**
    * Can use LOAD DATA LOCAL.
    *
    * Server
    * Supports LOAD DATA LOCAL.
    *
    * Client
    * Can use LOAD DATA LOCAL.
    */
   const val CLIENT_LOCAL_FILES: Int = 128

   /**
    * Ignore spaces before '('.
    *
    * Server
    * Parser can ignore spaces before '('.
    *
    * Client
    * Let the parser ignore spaces before '('.
    */
   const val CLIENT_IGNORE_SPACE: Int = 256

   /**
    * New 4.1 protocol.
    *
    * Server
    * Supports the 4.1 protocol.
    *
    * Client
    * Uses the 4.1 protocol.
    *
    * Note
    * this value was CLIENT_CHANGE_USER in 3.22, unused in 4.0
    */
   const val CLIENT_PROTOCOL_41: Int = 512

   /**
    * This is an interactive client.
    *
    * Use System_variables::net_wait_timeout versus System_variables::net_interactive_timeout.
    *
    * Server
    * Supports interactive and noninteractive clients.
    *
    * Client
    * Client is interactive.
    *
    * See also
    * mysql_real_connect()
    */
   const val CLIENT_INTERACTIVE: Int = 1024

   /**
    * Use SSL encryption for the session.
    *
    * Server
    * Supports SSL
    *
    * Client
    * Switch to SSL after sending the capability-flags.
    */
   const val CLIENT_SSL: Int = 2048

   /**
    * Client only flag.
    *
    * Not used.
    *
    * Client
    * Do not issue SIGPIPE if network failures occur (libmysqlclient only).
    *
    * See also
    * mysql_real_connect()
    */
   const val CLIENT_IGNORE_SIGPIPE: Int = 4096

   /**
    * Client knows about transactions.
    *
    * Server
    * Can send status flags in OK_Packet / EOF_Packet.
    *
    * Client
    * Expects status flags in OK_Packet / EOF_Packet.
    *
    * Note
    * This flag is optional in 3.23, but always set by the server since 4.0.
    * See also
    * send_server_handshake_packet(), parse_client_handshake_packet(), net_send_ok(), net_send_eof()
    */
   const val CLIENT_TRANSACTIONS: Int = 8192

   /**
    * DEPRECATED: Old flag for 4.1 protocol
    */
   const val CLIENT_RESERVED: Int = 16384

   /**
    * DEPRECATED: Old flag for 4.1 authentication \ CLIENT_SECURE_CONNECTION.
    */
   const val CLIENT_RESERVED2: Int = 32768

   /**
    * DEPRECATED: Old flag for 4.1 authentication \ CLIENT_SECURE_CONNECTION.
    */
   const val CLIENT_SECURE_CONNECTION: Int = 32768

   /**
    * Enable/disable multi-stmt support.
    *
    * Also sets CLIENT_MULTI_RESULTS. Currently not checked anywhere.
    *
    * Server
    * Can handle multiple statements per COM_QUERY and COM_STMT_PREPARE.
    *
    * Client
    * May send multiple statements per COM_QUERY and COM_STMT_PREPARE.
    *
    * Note
    * Was named CLIENT_MULTI_QUERIES in 4.1.0, renamed later.
    * Requires
    * CLIENT_PROTOCOL_41
    */
   const val CLIENT_MULTI_STATEMENTS: Int = 1 shl 16

   /**
    * Enable/disable multi-results.
    *
    * Server
    * Can send multiple resultsets for COM_QUERY. Error if the server needs to send them and client does not support them.
    *
    * Client
    * Can handle multiple resultsets for COM_QUERY.
    *
    * Requires
    * CLIENT_PROTOCOL_41
    *
    * See also
    * mysql_execute_command(), sp_head::MULTI_RESULTS
    */
   const val CLIENT_MULTI_RESULTS: Int = 1 shl 17

   /**
    * Multi-results and OUT parameters in PS-protocol.
    *
    * Server
    * Can send multiple resultsets for COM_STMT_EXECUTE.
    *
    * Client
    * Can handle multiple resultsets for COM_STMT_EXECUTE.
    *
    * Requires
    * CLIENT_PROTOCOL_41
    *
    * See also
    * Protocol_binary::send_out_parameters
    */
   const val CLIENT_PS_MULTI_RESULTS: Int = 1 shl 18

   /**
    * Client supports plugin authentication.
    *
    * Server
    * Sends extra data in Initial Handshake Packet and supports the pluggable
    * authentication protocol.
    *
    * Client
    * Supports authentication plugins.
    *
    * Requires
    * CLIENT_PROTOCOL_41
    *
    * See also
    * send_change_user_packet(), send_client_reply_packet(), run_plugin_auth(),
    * parse_com_change_user_packet(), parse_client_handshake_packet()
    */
   const val CLIENT_PLUGIN_AUTH: Int = 1 shl 19

   /**
    * Client supports connection attributes.
    *
    * Server
    * Permits connection attributes in Protocol::HandshakeResponse41.
    *
    * Client
    * Sends connection attributes in Protocol::HandshakeResponse41.
    *
    * See also
    * send_client_connect_attrs(), read_client_connect_attrs()
    */
   const val CLIENT_CONNECT_ATTRS: Int = 1 shl 20

   /**
    * Enable authentication response packet to be larger than 255 bytes.
    *
    * When the ability to change default plugin require that the initial password field in the Protocol::HandshakeResponse41 paclet can be of arbitrary size. However, the 4.1 client-server protocol limits the length of the auth-data-field sent from client to server to 255 bytes. The solution is to change the type of the field to a true length encoded string and indicate the protocol change with this client capability flag.
    *
    * Server
    * Understands length-encoded integer for auth response data in Protocol::HandshakeResponse41.
    *
    * Client
    * Length of auth response data in Protocol::HandshakeResponse41 is a length-encoded integer.
    *
    * Note
    * The flag was introduced in 5.6.6, but had the wrong value.
    * See also
    * send_client_reply_packet(), parse_client_handshake_packet(), get_56_lenc_string(), get_41_lenc_string()
    */
   const val CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA: Int = 1 shl 21

   /**
    * Don't close the connection for a user account with expired password.
    *
    * Server
    * Announces support for expired password extension.
    *
    * Client
    * Can handle expired passwords.
    *
    * See also
    * MYSQL_OPT_CAN_HANDLE_EXPIRED_PASSWORDS, disconnect_on_expired_password
    * ACL_USER::password_expired, check_password_lifetime(), acl_authenticate()
    */
   const val CLIENT_CAN_HANDLE_EXPIRED_PASSWORDS: Int = 1 shl 22

   /**
    * Capable of handling server state change information.
    *
    * Its a hint to the server to include the state change information in OK_Packet.
    *
    * Server
    * Can set SERVER_SESSION_STATE_CHANGED in the SERVER_STATUS_flags_enum and send
    * Session State Information in a OK_Packet.
    *
    * Client
    * Expects the server to send Session State Information in a OK_Packet.
    *
    * See also
    * enum_session_state_type, read_ok_ex(), net_send_ok(), Session_tracker, State_tracker
    */
   const val CLIENT_SESSION_TRACK: Int = 1 shl 23

   /**
    * Client no longer needs EOF_Packet and will use OK_Packet instead.
    *
    * See also
    * net_send_ok()
    * Server
    * Can send OK after a Text Resultset.
    *
    * Client
    * Expects an OK_Packet (instead of EOF_Packet) after the resultset rows of a Text Resultset.
    *
    * Background
    * To support CLIENT_SESSION_TRACK, additional information must be sent after all
    * successful commands. Although the OK_Packet is extensible, the EOF_Packet is not
    * due to the overlap of its bytes with the content of the Text Resultset Row.
    *
    * Therefore, the EOF_Packet in the Text Resultset is replaced with an OK_Packet.
    * EOF_Packet is deprecated as of MySQL 5.7.5.
    *
    * See also
    * cli_safe_read_with_ok(), read_ok_ex(), net_send_ok(), net_send_eof()
    */
   const val CLIENT_DEPRECATE_EOF: Int = 1 shl 24

   /**
    * The client can handle optional metadata information in the resultset.
    */
   const val CLIENT_OPTIONAL_RESULTSET_METADATA: Int = 1 shl 25

   /**
    * Compression protocol extended to support zstd compression method.
    *
    * This capability flag is used to send zstd compression level between client and
    * server provided both client and server are enabled with this flag.
    *
    * Server
    * Server sets this flag when global variable protocol-compression-algorithms has
    * zstd in its list of supported values.
    *
    * Client
    * Client sets this flag when it is configured to use zstd compression method.
    */
   const val CLIENT_ZSTD_COMPRESSION_ALGORITHM: Int = 1 shl 26

   /**
    * Support optional extension for query parameters into the COM_QUERY and
    * COM_STMT_EXECUTE packets.
    *
    * Server
    * Expects an optional part containing the query parameter set(s). Executes the
    * query for each set of parameters or returns an error if more than 1 set of
    * parameters is sent and the server can't execute it.
    *
    * Client
    * Can send the optional part containing the query parameter set(s).
    */
   const val CLIENT_QUERY_ATTRIBUTES: Int = 1 shl 27

   /**
    * Support Multi factor authentication.
    *
    * Server
    * Server sends AuthNextFactor packet after every nth factor authentication method succeeds,
    * except the last factor authentication.
    *
    * Client
    * Client reads AuthNextFactor packet sent by server and initiates next factor
    * authentication method.
    */
   const val MULTI_FACTOR_AUTHENTICATION: Int = 1 shl 28

   /**
    * This flag will be reserved to extend the 32bit capabilities structure to 64bits.
    */
   const val CLIENT_CAPABILITY_EXTENSION: Int = 1 shl 29

   /**
    * Verify server certificate.
    *
    * Client only flag.
    *
    * Deprecated:
    * in favor of –ssl-mode.
    */
   const val CLIENT_SSL_VERIFY_SERVER_CERT: Int = 1 shl 30

   /**
    * Don't reset the options after an unsuccessful connect.
    *
    * Client only flag.
    *
    * Typically passed via mysql_real_connect() 's client_flag parameter.
    *
    * See also
    * mysql_real_connect()
    */
   const val CLIENT_REMEMBER_OPTIONS: Int = 1 shl 31
}

object ComRefreshFlags {
   /**
    * Refresh grant tables, FLUSH PRIVILEGES.
    */
   const val REFRESH_GRANT: Int = 1

   /**
    * Start on new log file, FLUSH LOGS.
    */
   const val REFRESH_LOG: Int = 2

   /**
    * close all tables, FLUSH TABLES
    */
   const val REFRESH_TABLES: Int = 4

   /**
    * Previously REFRESH_HOSTS but not used anymore.
    *
    * Use TRUNCATE TABLE \ performance_schema.host_cache instead
    */
   const val UNUSED: Int = 8

   /**
    * Flush status variables, FLUSH STATUS.
    */
   const val REFRESH_STATUS: Int = 16

   /**
    * Removed.
    *
    * Used to be flush thread cache
    */
   const val UNUSED_32: Int = 32

   /**
    * Reset source info and restart replica \ thread, RESET REPLICA.
    */
   const val REFRESH_REPLICA: Int = 64

   /**
    * Remove all bin logs in the index \ and truncate the index.
    *
    * Also resets \ GTID information. Command: \ RESET BINARY LOGS AND GTIDS
    */
   const val REFRESH_SOURCE: Int = 128

   /**
    * Rotate only the error log.
    */
   const val REFRESH_ERROR_LOG: Int = 256

   /**
    * Flush all storage engine logs.
    */
   const val REFRESH_ENGINE_LOG: Int = 512

   /**
    * Flush the binary log.
    */
   const val REFRESH_BINARY_LOG: Int = 1024

   /**
    * Flush the relay log.
    */
   const val REFRESH_RELAY_LOG: Int = 2048

   /**
    * Flush the general log.
    */
   const val REFRESH_GENERAL_LOG: Int = 4096

   /**
    * Flush the slow query log.
    */
   const val REFRESH_SLOW_LOG: Int = 8192

   /**
    * Lock tables for read.
    */
   const val REFRESH_READ_LOCK: Int = 16384

   /**
    * Wait for an impending flush before closing the tables.
    *
    * See also
    * REFRESH_READ_LOCK, handle_reload_request, close_cached_tables
    */
   const val REFRESH_FAST: Int = 32768
   const val REFRESH_FOR_EXPORT: Int = 0x100000
   const val REFRESH_OPTIMIZER_COSTS: Int = 0x200000
   const val REFRESH_PERSIST: Int = 0x400000
}

object BinlogEventHeaderFlags {
   /**
    * If the query depends on the thread (for example: TEMPORARY TABLE).
    *
    * Currently this is used by mysqlbinlog to know it must print SET @PSEUDO_THREAD_ID=xx;
    * before the query (it would not hurt to print it for every query but this would be slow).
    */
   const val LOG_EVENT_THREAD_SPECIFIC_F: Int = 0x4

   /**
    * Suppress the generation of 'USE' statements before the actual statement.
    *
    * This flag should be set for any events that does not need the current database
    * set to function correctly. Most notable cases are 'CREATE DATABASE' and 'DROP DATABASE'.
    *
    * This flags should only be used in exceptional circumstances, since it introduce a
    * significant change in behaviour regarding the replication logic together with the
    * flags –binlog-do-db and –replicated-do-db.
    */
   const val LOG_EVENT_SUPPRESS_USE_F: Int  = 0x8

   /**
    * Artificial events are created arbitrarily and not written to binary log.
    *
    * These events should not update the master log position when slave SQL thread executes them.
    */
   const val LOG_EVENT_ARTIFICIAL_F: Int  = 0x20

   /**
    * Events with this flag set are created by slave IO thread and written to relay log.
    */
   const val LOG_EVENT_RELAY_LOG_F: Int  = 0x40

   /**
    * For an event, 'e', carrying a type code, that a slave, 's', does not recognize,
    * 's' will check 'e' for LOG_EVENT_IGNORABLE_F, and if the flag is set, then 'e' is ignored.
    *
    * Otherwise, 's' acknowledges that it has found an unknown event in the relay log.
    */
   const val LOG_EVENT_IGNORABLE_F: Int  = 0x80

   /**
    * Events with this flag are not filtered (e.g.
    *
    * on the current database) and are always written to the binary log regardless of filters.
    */
   const val LOG_EVENT_NO_FILTER_F: Int  = 0x100

   /**
    * MTS: group of events can be marked to force its execution in isolation from any other Workers.
    *
    * So it's a marker for Coordinator to memorize and perform necessary operations in order to
    * guarantee no interference from other Workers. The flag can be set ON only for an event that
    * terminates its group. Typically that is done for a transaction that contains a query
    * accessing more than OVER_MAX_DBS_IN_EVENT_MTS databases.
    */
   const val LOG_EVENT_MTS_ISOLATE_F: Int  = 0x200
}

object ShutdownFlags {
   const val MYSQL_SHUTDOWN_KILLABLE_CONNECT: Int = 1 shl 0
   const val MYSQL_SHUTDOWN_KILLABLE_TRANS: Int = 1 shl 1
   const val MYSQL_SHUTDOWN_KILLABLE_LOCK_TABLE: Int = 1 shl 2
   const val MYSQL_SHUTDOWN_KILLABLE_UPDATE: Int = 1 shl 3
}

/**
 * We want levels to be in growing order of hardness (because we use number comparisons).
 *
 * Note
 * SHUTDOWN_DEFAULT does not respect the growing property, but it's ok.
 */
enum class ShutdownLevel {
   SHUTDOWN_DEFAULT,

   /**
    * Wait for existing connections to finish.
    */
   SHUTDOWN_WAIT_CONNECTIONS,

   /**
    * Wait for existing transactons to finish.
    */
   SHUTDOWN_WAIT_TRANSACTIONS,

   /**
    * Wait for existing updates to finish (=> no partial MyISAM update)
    */
   SHUTDOWN_WAIT_UPDATES,

   /**
    * Flush InnoDB buffers and other storage engines' buffers.
    */
   SHUTDOWN_WAIT_ALL_BUFFERS,

   /**
    * Don't flush InnoDB buffers, flush other storage engines' buffers.
    */
   SHUTDOWN_WAIT_CRITICAL_BUFFERS,

   /**
    * Query level of the KILL command.
    */
   KILL_QUERY,

   /**
    * Connection level of the KILL command.
    */
   KILL_CONNECTION;
}

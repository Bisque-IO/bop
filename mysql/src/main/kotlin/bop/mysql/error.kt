package bop.mysql

/**
 * MySQL error codes. Auto-generated from sql/share/errmsg-utf8.txt.
 */
object ErrorCode {
   /**
    * Hashchk
    */
   const val ER_HASHCHK: Int = 1000

   /**
    * Isamchk
    */
   const val ER_NISAMCHK: Int = 1001

   /**
    * NO
    */
   const val ER_NO: Int = 1002

   /**
    * YES
    */
   const val ER_YES: Int = 1003

   /**
    * Can't create file '%-.200s' (errno: %d - %s)
    */
   const val ER_CANT_CREATE_FILE: Int = 1004

   /**
    * Can't create table '%-.200s' (errno: %d)
    */
   const val ER_CANT_CREATE_TABLE: Int = 1005

   /**
    * Can't create database '%-.192s' (errno: %d)
    */
   const val ER_CANT_CREATE_DB: Int = 1006

   /**
    * Can't create database '%-.192s'; database exists
    */
   const val ER_DB_CREATE_EXISTS: Int = 1007

   /**
    * Can't drop database '%-.192s'; database doesn't exist
    */
   const val ER_DB_DROP_EXISTS: Int = 1008

   /**
    * Error dropping database (can't delete '%-.192s', errno: %d)
    */
   const val ER_DB_DROP_DELETE: Int = 1009

   /**
    * Error dropping database (can't rmdir '%-.192s', errno: %d)
    */
   const val ER_DB_DROP_RMDIR: Int = 1010

   /**
    * Error on delete of '%-.192s' (errno: %d - %s)
    */
   const val ER_CANT_DELETE_FILE: Int = 1011

   /**
    * Can't read record in system table
    */
   const val ER_CANT_FIND_SYSTEM_REC: Int = 1012

   /**
    * Can't get status of '%-.200s' (errno: %d - %s)
    */
   const val ER_CANT_GET_STAT: Int = 1013

   /**
    * Can't get working directory (errno: %d - %s)
    */
   const val ER_CANT_GET_WD: Int = 1014

   /**
    * Can't lock file (errno: %d - %s)
    */
   const val ER_CANT_LOCK: Int = 1015

   /**
    * Can't open file: '%-.200s' (errno: %d - %s)
    */
   const val ER_CANT_OPEN_FILE: Int = 1016

   /**
    * Can't find file: '%-.200s' (errno: %d - %s)
    */
   const val ER_FILE_NOT_FOUND: Int = 1017

   /**
    * Can't read dir of '%-.192s' (errno: %d - %s)
    */
   const val ER_CANT_READ_DIR: Int = 1018

   /**
    * Can't change dir to '%-.192s' (errno: %d - %s)
    */
   const val ER_CANT_SET_WD: Int = 1019

   /**
    * Record has changed since last read in table '%-.192s'
    */
   const val ER_CHECKREAD: Int = 1020

   /**
    * Disk full (%s); waiting for someone to free some space... (errno: %d - %s)
    */
   const val ER_DISK_FULL: Int = 1021

   /**
    * Can't write; duplicate key in table '%-.192s'
    */
   const val ER_DUP_KEY: Int = 1022

   /**
    * Error on close of '%-.192s' (errno: %d - %s)
    */
   const val ER_ERROR_ON_CLOSE: Int = 1023

   /**
    * Error reading file '%-.200s' (errno: %d - %s)
    */
   const val ER_ERROR_ON_READ: Int = 1024

   /**
    * Error on rename of '%-.210s' to '%-.210s' (errno: %d - %s)
    */
   const val ER_ERROR_ON_RENAME: Int = 1025

   /**
    * Error writing file '%-.200s' (errno: %d - %s)
    */
   const val ER_ERROR_ON_WRITE: Int = 1026

   /**
    * '%-.192s' is locked against change
    */
   const val ER_FILE_USED: Int = 1027

   /**
    * Sort aborted
    */
   const val ER_FILSORT_ABORT: Int = 1028

   /**
    * View '%-.192s' doesn't exist for '%-.192s'
    */
   const val ER_FORM_NOT_FOUND: Int = 1029

   /**
    * Got error %d from storage engine
    */
   const val ER_GET_ERRNO: Int = 1030

   /**
    * Table storage engine for '%-.192s' doesn't have this option
    */
   const val ER_ILLEGAL_HA: Int = 1031

   /**
    * Can't find record in '%-.192s'
    */
   const val ER_KEY_NOT_FOUND: Int = 1032

   /**
    * Incorrect information in file: '%-.200s'
    */
   const val ER_NOT_FORM_FILE: Int = 1033

   /**
    * Incorrect key file for table '%-.200s'; try to repair it
    */
   const val ER_NOT_KEYFILE: Int = 1034

   /**
    * Old key file for table '%-.192s'; repair it!
    */
   const val ER_OLD_KEYFILE: Int = 1035

   /**
    * Table '%-.192s' is read only
    */
   const val ER_OPEN_AS_READONLY: Int = 1036

   /**
    * Out of memory; restart server and try again (needed %d bytes)
    */
   const val ER_OUTOFMEMORY: Int = 1037

   /**
    * Out of sort memory, consider increasing server sort buffer size
    */
   const val ER_OUT_OF_SORTMEMORY: Int = 1038

   /**
    * Unexpected EOF found when reading file '%-.192s' (errno: %d - %s)
    */
   const val ER_UNEXPECTED_EOF: Int = 1039

   /**
    * Too many connections
    */
   const val ER_CON_COUNT_ERROR: Int = 1040

   /**
    * Out of memory; check if mysqld or some other process uses all available memory; if not, you may have to use
    * 'ulimit' to allow mysqld to use more memory or you can add more swap space
    */
   const val ER_OUT_OF_RESOURCES: Int = 1041

   /**
    * Can't get hostname for your address
    */
   const val ER_BAD_HOST_ERROR: Int = 1042

   /**
    * Bad handshake
    */
   const val ER_HANDSHAKE_ERROR: Int = 1043

   /**
    * Access denied for user '%-.48s'@'%-.64s' to database '%-.192s'
    */
   const val ER_DBACCESS_DENIED_ERROR: Int = 1044

   /**
    * Access denied for user '%-.48s'@'%-.64s' (using password: %s)
    */
   const val ER_ACCESS_DENIED_ERROR: Int = 1045

   /**
    * No database selected
    */
   const val ER_NO_DB_ERROR: Int = 1046

   /**
    * Unknown command
    */
   const val ER_UNKNOWN_COM_ERROR: Int = 1047

   /**
    * Column '%-.192s' cannot be null
    */
   const val ER_BAD_NULL_ERROR: Int = 1048

   /**
    * Unknown database '%-.192s'
    */
   const val ER_BAD_DB_ERROR: Int = 1049

   /**
    * Table '%-.192s' already exists
    */
   const val ER_TABLE_EXISTS_ERROR: Int = 1050

   /**
    * Unknown table '%-.100s'
    */
   const val ER_BAD_TABLE_ERROR: Int = 1051

   /**
    * Column '%-.192s' in %-.192s is ambiguous
    */
   const val ER_NON_UNIQ_ERROR: Int = 1052

   /**
    * Server shutdown in progress
    */
   const val ER_SERVER_SHUTDOWN: Int = 1053

   /**
    * Unknown column '%-.192s' in '%-.192s'
    */
   const val ER_BAD_FIELD_ERROR: Int = 1054

   /**
    * '%-.192s' isn't in GROUP BY
    */
   const val ER_WRONG_FIELD_WITH_GROUP: Int = 1055

   /**
    * Can't group on '%-.192s'
    */
   const val ER_WRONG_GROUP_FIELD: Int = 1056

   /**
    * Statement has sum functions and columns in same statement
    */
   const val ER_WRONG_SUM_SELECT: Int = 1057

   /**
    * Column count doesn't match value count
    */
   const val ER_WRONG_VALUE_COUNT: Int = 1058

   /**
    * Identifier name '%-.100s' is too long
    */
   const val ER_TOO_LONG_IDENT: Int = 1059

   /**
    * Duplicate column name '%-.192s'
    */
   const val ER_DUP_FIELDNAME: Int = 1060

   /**
    * Duplicate key name '%-.192s'
    */
   const val ER_DUP_KEYNAME: Int = 1061

   /**
    * Duplicate entry '%-.192s' for key %d
    */
   const val ER_DUP_ENTRY: Int = 1062

   /**
    * Incorrect column specifier for column '%-.192s'
    */
   const val ER_WRONG_FIELD_SPEC: Int = 1063

   /**
    * %s near '%-.80s' at line %d
    */
   const val ER_PARSE_ERROR: Int = 1064

   /**
    * Query was empty
    */
   const val ER_EMPTY_QUERY: Int = 1065

   /**
    * Not unique table/alias: '%-.192s'
    */
   const val ER_NONUNIQ_TABLE: Int = 1066

   /**
    * Invalid default value for '%-.192s'
    */
   const val ER_INVALID_DEFAULT: Int = 1067

   /**
    * Multiple primary key defined
    */
   const val ER_MULTIPLE_PRI_KEY: Int = 1068

   /**
    * Too many keys specified; max %d keys allowed
    */
   const val ER_TOO_MANY_KEYS: Int = 1069

   /**
    * Too many key parts specified; max %d parts allowed
    */
   const val ER_TOO_MANY_KEY_PARTS: Int = 1070

   /**
    * Specified key was too long; max key length is %d bytes
    */
   const val ER_TOO_LONG_KEY: Int = 1071

   /**
    * Key column '%-.192s' doesn't exist in table
    */
   const val ER_KEY_COLUMN_DOES_NOT_EXITS: Int = 1072

   /**
    * BLOB column '%-.192s' can't be used in key specification with the used table type
    */
   const val ER_BLOB_USED_AS_KEY: Int = 1073

   /**
    * Column length too big for column '%-.192s' (max = %lu); use BLOB or TEXT instead
    */
   const val ER_TOO_BIG_FIELDLENGTH: Int = 1074

   /**
    * Incorrect table definition; there can be only one auto column and it must be defined as a key
    */
   const val ER_WRONG_AUTO_KEY: Int = 1075

   /**
    * %s: ready for connections.\nVersion: '%s'  socket: '%s'  port: %d
    */
   const val ER_READY: Int = 1076

   /**
    * %s: Normal shutdown\n
    */
   const val ER_NORMAL_SHUTDOWN: Int = 1077

   /**
    * %s: Got signal %d. Aborting!\n
    */
   const val ER_GOT_SIGNAL: Int = 1078

   /**
    * %s: Shutdown complete\n
    */
   const val ER_SHUTDOWN_COMPLETE: Int = 1079

   /**
    * %s: Forcing close of thread %ld  user: '%-.48s'\n
    */
   const val ER_FORCING_CLOSE: Int = 1080

   /**
    * Can't create IP socket
    */
   const val ER_IPSOCK_ERROR: Int = 1081

   /**
    * Table '%-.192s' has no index like the one used in CREATE INDEX; recreate the table
    */
   const val ER_NO_SUCH_INDEX: Int = 1082

   /**
    * Field separator argument is not what is expected; check the manual
    */
   const val ER_WRONG_FIELD_TERMINATORS: Int = 1083

   /**
    * You can't use fixed rowlength with BLOBs; please use 'fields terminated by'
    */
   const val ER_BLOBS_AND_NO_TERMINATED: Int = 1084

   /**
    * The file '%-.128s' must be in the database directory or be readable by all
    */
   const val ER_TEXTFILE_NOT_READABLE: Int = 1085

   /**
    * File '%-.200s' already exists
    */
   const val ER_FILE_EXISTS_ERROR: Int = 1086

   /**
    * Records: %ld  Deleted: %ld  Skipped: %ld  Warnings: %ld
    */
   const val ER_LOAD_INFO: Int = 1087

   /**
    * Records: %ld  Duplicates: %ld
    */
   const val ER_ALTER_INFO: Int = 1088

   /**
    * Incorrect prefix key; the used key part isn't a string, the used length is longer than the key part, or the
    * storage engine doesn't support unique prefix keys
    */
   const val ER_WRONG_SUB_KEY: Int = 1089

   /**
    * You can't delete all columns with ALTER TABLE; use DROP TABLE instead
    */
   const val ER_CANT_REMOVE_ALL_FIELDS: Int = 1090

   /**
    * Can't DROP '%-.192s'; check that column/key exists
    */
   const val ER_CANT_DROP_FIELD_OR_KEY: Int = 1091

   /**
    * Records: %ld  Duplicates: %ld  Warnings: %ld
    */
   const val ER_INSERT_INFO: Int = 1092

   /**
    * You can't specify target table '%-.192s' for update in FROM clause
    */
   const val ER_UPDATE_TABLE_USED: Int = 1093

   /**
    * Unknown thread id: %lu
    */
   const val ER_NO_SUCH_THREAD: Int = 1094

   /**
    * You are not owner of thread %lu
    */
   const val ER_KILL_DENIED_ERROR: Int = 1095

   /**
    * No tables used
    */
   const val ER_NO_TABLES_USED: Int = 1096

   /**
    * Too many strings for column %-.192s and SET
    */
   const val ER_TOO_BIG_SET: Int = 1097

   /**
    * Can't generate a unique log-filename %-.200s.(1-999)\n
    */
   const val ER_NO_UNIQUE_LOGFILE: Int = 1098

   /**
    * Table '%-.192s' was locked with a READ lock and can't be updated
    */
   const val ER_TABLE_NOT_LOCKED_FOR_WRITE: Int = 1099

   /**
    * Table '%-.192s' was not locked with LOCK TABLES
    */
   const val ER_TABLE_NOT_LOCKED: Int = 1100

   /**
    * BLOB/TEXT column '%-.192s' can't have a default value
    */
   const val ER_BLOB_CANT_HAVE_DEFAULT: Int = 1101

   /**
    * Incorrect database name '%-.100s'
    */
   const val ER_WRONG_DB_NAME: Int = 1102

   /**
    * Incorrect table name '%-.100s'
    */
   const val ER_WRONG_TABLE_NAME: Int = 1103

   /**
    * The SELECT would examine more than MAX_JOIN_SIZE rows; check your WHERE and use SET SQL_BIG_SELECTS=1 or SET
    * MAX_JOIN_SIZE=# if the SELECT is okay
    */
   const val ER_TOO_BIG_SELECT: Int = 1104

   /**
    * Unknown error
    */
   const val ER_UNKNOWN_ERROR: Int = 1105

   /**
    * Unknown procedure '%-.192s'
    */
   const val ER_UNKNOWN_PROCEDURE: Int = 1106

   /**
    * Incorrect parameter count to procedure '%-.192s'
    */
   const val ER_WRONG_PARAMCOUNT_TO_PROCEDURE: Int = 1107

   /**
    * Incorrect parameters to procedure '%-.192s'
    */
   const val ER_WRONG_PARAMETERS_TO_PROCEDURE: Int = 1108

   /**
    * Unknown table '%-.192s' in %-.32s
    */
   const val ER_UNKNOWN_TABLE: Int = 1109

   /**
    * Column '%-.192s' specified twice
    */
   const val ER_FIELD_SPECIFIED_TWICE: Int = 1110

   /**
    * Invalid use of group function
    */
   const val ER_INVALID_GROUP_FUNC_USE: Int = 1111

   /**
    * Table '%-.192s' uses an extension that doesn't exist in this MySQL version
    */
   const val ER_UNSUPPORTED_EXTENSION: Int = 1112

   /**
    * A table must have at least 1 column
    */
   const val ER_TABLE_MUST_HAVE_COLUMNS: Int = 1113

   /**
    * The table '%-.192s' is full
    */
   const val ER_RECORD_FILE_FULL: Int = 1114

   /**
    * Unknown character set: '%-.64s'
    */
   const val ER_UNKNOWN_CHARACTER_SET: Int = 1115

   /**
    * Too many tables; MySQL can only use %d tables in a join
    */
   const val ER_TOO_MANY_TABLES: Int = 1116

   /**
    * Too many columns
    */
   const val ER_TOO_MANY_FIELDS: Int = 1117

   /**
    * Row size too large. The maximum row size for the used table type, not counting BLOBs, is %ld. This includes
    * storage overhead, check the manual. You have to change some columns to TEXT or BLOBs
    */
   const val ER_TOO_BIG_ROWSIZE: Int = 1118

   /**
    * Thread stack overrun:  Used: %ld of a %ld stack.  Use 'mysqld --thread_stack=#' to specify a bigger stack if
    * needed
    */
   const val ER_STACK_OVERRUN: Int = 1119

   /**
    * Cross dependency found in OUTER JOIN; examine your ON conditions
    */
   const val ER_WRONG_OUTER_JOIN: Int = 1120

   /**
    * Table handler doesn't support NULL in given index. Please change column '%-.192s' to be NOT NULL or use
    * another handler
    */
   const val ER_NULL_COLUMN_IN_INDEX: Int = 1121

   /**
    * Can't load function '%-.192s'
    */
   const val ER_CANT_FIND_UDF: Int = 1122

   /**
    * Can't initialize function '%-.192s'; %-.80s
    */
   const val ER_CANT_INITIALIZE_UDF: Int = 1123

   /**
    * No paths allowed for shared library
    */
   const val ER_UDF_NO_PATHS: Int = 1124

   /**
    * Function '%-.192s' already exists
    */
   const val ER_UDF_EXISTS: Int = 1125

   /**
    * Can't open shared library '%-.192s' (errno: %d %-.128s)
    */
   const val ER_CANT_OPEN_LIBRARY: Int = 1126

   /**
    * Can't find symbol '%-.128s' in library
    */
   const val ER_CANT_FIND_DL_ENTRY: Int = 1127

   /**
    * Function '%-.192s' is not defined
    */
   const val ER_FUNCTION_NOT_DEFINED: Int = 1128

   /**
    * Host '%-.64s' is blocked because of many connection errors; unblock with 'mysqladmin flush-hosts'
    */
   const val ER_HOST_IS_BLOCKED: Int = 1129

   /**
    * Host '%-.64s' is not allowed to connect to this MySQL server
    */
   const val ER_HOST_NOT_PRIVILEGED: Int = 1130

   /**
    * You are using MySQL as an anonymous user and anonymous users are not allowed to change passwords
    */
   const val ER_PASSWORD_ANONYMOUS_USER: Int = 1131

   /**
    * You must have privileges to update tables in the mysql database to be able to change passwords for others
    */
   const val ER_PASSWORD_NOT_ALLOWED: Int = 1132

   /**
    * Can't find any matching row in the user table
    */
   const val ER_PASSWORD_NO_MATCH: Int = 1133

   /**
    * Rows matched: %ld  Changed: %ld  Warnings: %ld
    */
   const val ER_UPDATE_INFO: Int = 1134

   /**
    * Can't create a new thread (errno %d); if you are not out of available memory, you can consult the manual for a
    * possible OS-dependent bug
    */
   const val ER_CANT_CREATE_THREAD: Int = 1135

   /**
    * Column count doesn't match value count at row %ld
    */
   const val ER_WRONG_VALUE_COUNT_ON_ROW: Int = 1136

   /**
    * Can't reopen table: '%-.192s'
    */
   const val ER_CANT_REOPEN_TABLE: Int = 1137

   /**
    * Invalid use of NULL value
    */
   const val ER_INVALID_USE_OF_NULL: Int = 1138

   /**
    * Got error '%-.64s' from regexp
    */
   const val ER_REGEXP_ERROR: Int = 1139

   /**
    * Mixing of GROUP columns (MIN(),MAX(),COUNT(),...) with no GROUP columns is illegal if there is no GROUP BY
    * clause
    */
   const val ER_MIX_OF_GROUP_FUNC_AND_FIELDS: Int = 1140

   /**
    * There is no such grant defined for user '%-.48s' on host '%-.64s'
    */
   const val ER_NONEXISTING_GRANT: Int = 1141

   /**
    * %-.128s command denied to user '%-.48s'@'%-.64s' for table '%-.64s'
    */
   const val ER_TABLEACCESS_DENIED_ERROR: Int = 1142

   /**
    * %-.16s command denied to user '%-.48s'@'%-.64s' for column '%-.192s' in table '%-.192s'
    */
   const val ER_COLUMNACCESS_DENIED_ERROR: Int = 1143

   /**
    * Illegal GRANT/REVOKE command; please consult the manual to see which privileges can be used
    */
   const val ER_ILLEGAL_GRANT_FOR_TABLE: Int = 1144

   /**
    * The host or user argument to GRANT is too long
    */
   const val ER_GRANT_WRONG_HOST_OR_USER: Int = 1145

   /**
    * Table '%-.192s.%-.192s' doesn't exist
    */
   const val ER_NO_SUCH_TABLE: Int = 1146

   /**
    * There is no such grant defined for user '%-.48s' on host '%-.64s' on table '%-.192s'
    */
   const val ER_NONEXISTING_TABLE_GRANT: Int = 1147

   /**
    * The used command is not allowed with this MySQL version
    */
   const val ER_NOT_ALLOWED_COMMAND: Int = 1148

   /**
    * You have an error in your SQL syntax; check the manual that corresponds to your MySQL server version for the
    * right syntax to use
    */
   const val ER_SYNTAX_ERROR: Int = 1149

   /**
    * Delayed insert thread couldn't get requested lock for table %-.192s
    */
   const val ER_DELAYED_CANT_CHANGE_LOCK: Int = 1150

   /**
    * Too many delayed threads in use
    */
   const val ER_TOO_MANY_DELAYED_THREADS: Int = 1151

   /**
    * Aborted connection %ld to db: '%-.192s' user: '%-.48s' (%-.64s)
    */
   const val ER_ABORTING_CONNECTION: Int = 1152

   /**
    * Got a packet bigger than 'max_allowed_packet' bytes
    */
   const val ER_NET_PACKET_TOO_LARGE: Int = 1153

   /**
    * Got a read error from the connection pipe
    */
   const val ER_NET_READ_ERROR_FROM_PIPE: Int = 1154

   /**
    * Got an error from fcntl()
    */
   const val ER_NET_FCNTL_ERROR: Int = 1155

   /**
    * Got packets out of order
    */
   const val ER_NET_PACKETS_OUT_OF_ORDER: Int = 1156

   /**
    * Couldn't uncompress communication packet
    */
   const val ER_NET_UNCOMPRESS_ERROR: Int = 1157

   /**
    * Got an error reading communication packets
    */
   const val ER_NET_READ_ERROR: Int = 1158

   /**
    * Got timeout reading communication packets
    */
   const val ER_NET_READ_INTERRUPTED: Int = 1159

   /**
    * Got an error writing communication packets
    */
   const val ER_NET_ERROR_ON_WRITE: Int = 1160

   /**
    * Got timeout writing communication packets
    */
   const val ER_NET_WRITE_INTERRUPTED: Int = 1161

   /**
    * Result string is longer than 'max_allowed_packet' bytes
    */
   const val ER_TOO_LONG_STRING: Int = 1162

   /**
    * The used table type doesn't support BLOB/TEXT columns
    */
   const val ER_TABLE_CANT_HANDLE_BLOB: Int = 1163

   /**
    * The used table type doesn't support AUTO_INCREMENT columns
    */
   const val ER_TABLE_CANT_HANDLE_AUTO_INCREMENT: Int = 1164

   /**
    * INSERT DELAYED can't be used with table '%-.192s' because it is locked with LOCK TABLES
    */
   const val ER_DELAYED_INSERT_TABLE_LOCKED: Int = 1165

   /**
    * Incorrect column name '%-.100s'
    */
   const val ER_WRONG_COLUMN_NAME: Int = 1166

   /**
    * The used storage engine can't index column '%-.192s'
    */
   const val ER_WRONG_KEY_COLUMN: Int = 1167

   /**
    * Unable to open underlying table which is differently defined or of non-MyISAM type or doesn't exist
    */
   const val ER_WRONG_MRG_TABLE: Int = 1168

   /**
    * Can't write, because of unique constraint, to table '%-.192s'
    */
   const val ER_DUP_UNIQUE: Int = 1169

   /**
    * BLOB/TEXT column '%-.192s' used in key specification without a key length
    */
   const val ER_BLOB_KEY_WITHOUT_LENGTH: Int = 1170

   /**
    * All parts of a PRIMARY KEY must be NOT NULL; if you need NULL in a key, use UNIQUE instead
    */
   const val ER_PRIMARY_CANT_HAVE_NULL: Int = 1171

   /**
    * Result consisted of more than one row
    */
   const val ER_TOO_MANY_ROWS: Int = 1172

   /**
    * This table type requires a primary key
    */
   const val ER_REQUIRES_PRIMARY_KEY: Int = 1173

   /**
    * This version of MySQL is not compiled with RAID support
    */
   const val ER_NO_RAID_COMPILED: Int = 1174

   /**
    * You are using safe update mode and you tried to update a table without a WHERE that uses a KEY column
    */
   const val ER_UPDATE_WITHOUT_KEY_IN_SAFE_MODE: Int = 1175

   /**
    * Key '%-.192s' doesn't exist in table '%-.192s'
    */
   const val ER_KEY_DOES_NOT_EXITS: Int = 1176

   /**
    * Can't open table
    */
   const val ER_CHECK_NO_SUCH_TABLE: Int = 1177

   /**
    * The storage engine for the table doesn't support %s
    */
   const val ER_CHECK_NOT_IMPLEMENTED: Int = 1178

   /**
    * You are not allowed to execute this command in a transaction
    */
   const val ER_CANT_DO_THIS_DURING_AN_TRANSACTION: Int = 1179

   /**
    * Got error %d during COMMIT
    */
   const val ER_ERROR_DURING_COMMIT: Int = 1180

   /**
    * Got error %d during ROLLBACK
    */
   const val ER_ERROR_DURING_ROLLBACK: Int = 1181

   /**
    * Got error %d during FLUSH_LOGS
    */
   const val ER_ERROR_DURING_FLUSH_LOGS: Int = 1182

   /**
    * Got error %d during CHECKPOINT
    */
   const val ER_ERROR_DURING_CHECKPOINT: Int = 1183

   /**
    * Aborted connection %ld to db: '%-.192s' user: '%-.48s' host: '%-.64s' (%-.64s)
    */
   const val ER_NEW_ABORTING_CONNECTION: Int = 1184

   /**
    * The storage engine for the table does not support binary table dump
    */
   const val ER_DUMP_NOT_IMPLEMENTED: Int = 1185

   /**
    * Binlog closed, cannot RESET MASTER
    */
   const val ER_FLUSH_MASTER_BINLOG_CLOSED: Int = 1186

   /**
    * Failed rebuilding the index of  dumped table '%-.192s'
    */
   const val ER_INDEX_REBUILD: Int = 1187

   /**
    * Error from master: '%-.64s'
    */
   const val ER_MASTER: Int = 1188

   /**
    * Net error reading from master
    */
   const val ER_MASTER_NET_READ: Int = 1189

   /**
    * Net error writing to master
    */
   const val ER_MASTER_NET_WRITE: Int = 1190

   /**
    * Can't find FULLTEXT index matching the column list
    */
   const val ER_FT_MATCHING_KEY_NOT_FOUND: Int = 1191

   /**
    * Can't execute the given command because you have active locked tables or an active transaction
    */
   const val ER_LOCK_OR_ACTIVE_TRANSACTION: Int = 1192

   /**
    * Unknown system variable '%-.64s'
    */
   const val ER_UNKNOWN_SYSTEM_VARIABLE: Int = 1193

   /**
    * Table '%-.192s' is marked as crashed and should be repaired
    */
   const val ER_CRASHED_ON_USAGE: Int = 1194

   /**
    * Table '%-.192s' is marked as crashed and last (automatic?) repair failed
    */
   const val ER_CRASHED_ON_REPAIR: Int = 1195

   /**
    * Some non-transactional changed tables couldn't be rolled back
    */
   const val ER_WARNING_NOT_COMPLETE_ROLLBACK: Int = 1196

   /**
    * Multi-statement transaction required more than 'max_binlog_cache_size' bytes of storage; increase this mysqld
    * variable and try again
    */
   const val ER_TRANS_CACHE_FULL: Int = 1197

   /**
    * This operation cannot be performed with a running slave; run STOP SLAVE first
    */
   const val ER_SLAVE_MUST_STOP: Int = 1198

   /**
    * This operation requires a running slave; configure slave and do START SLAVE
    */
   const val ER_SLAVE_NOT_RUNNING: Int = 1199

   /**
    * The server is not configured as slave; fix in config file or with CHANGE MASTER TO
    */
   const val ER_BAD_SLAVE: Int = 1200

   /**
    * Could not initialize master info structure; more error messages can be found in the MySQL error log
    */
   const val ER_MASTER_INFO: Int = 1201

   /**
    * Could not create slave thread; check system resources
    */
   const val ER_SLAVE_THREAD: Int = 1202

   /**
    * User %-.64s already has more than 'max_user_connections' active connections
    */
   const val ER_TOO_MANY_USER_CONNECTIONS: Int = 1203

   /**
    * You may only use constant expressions with SET
    */
   const val ER_SET_CONSTANTS_ONLY: Int = 1204

   /**
    * Lock wait timeout exceeded; try restarting transaction
    */
   const val ER_LOCK_WAIT_TIMEOUT: Int = 1205

   /**
    * The total number of locks exceeds the lock table size
    */
   const val ER_LOCK_TABLE_FULL: Int = 1206

   /**
    * Update locks cannot be acquired during a READ UNCOMMITTED transaction
    */
   const val ER_READ_ONLY_TRANSACTION: Int = 1207

   /**
    * DROP DATABASE not allowed while thread is holding global read lock
    */
   const val ER_DROP_DB_WITH_READ_LOCK: Int = 1208

   /**
    * CREATE DATABASE not allowed while thread is holding global read lock
    */
   const val ER_CREATE_DB_WITH_READ_LOCK: Int = 1209

   /**
    * Incorrect arguments to %s
    */
   const val ER_WRONG_ARGUMENTS: Int = 1210

   /**
    * '%-.48s'@'%-.64s' is not allowed to create new users
    */
   const val ER_NO_PERMISSION_TO_CREATE_USER: Int = 1211

   /**
    * Incorrect table definition; all MERGE tables must be in the same database
    */
   const val ER_UNION_TABLES_IN_DIFFERENT_DIR: Int = 1212

   /**
    * Deadlock found when trying to get lock; try restarting transaction
    */
   const val ER_LOCK_DEADLOCK: Int = 1213

   /**
    * The used table type doesn't support FULLTEXT indexes
    */
   const val ER_TABLE_CANT_HANDLE_FT: Int = 1214

   /**
    * Cannot add foreign key constraint
    */
   const val ER_CANNOT_ADD_FOREIGN: Int = 1215

   /**
    * Cannot add or update a child row: a foreign key constraint fails
    */
   const val ER_NO_REFERENCED_ROW: Int = 1216

   /**
    * Cannot delete or update a parent row: a foreign key constraint fails
    */
   const val ER_ROW_IS_REFERENCED: Int = 1217

   /**
    * Error connecting to master: %-.128s
    */
   const val ER_CONNECT_TO_MASTER: Int = 1218

   /**
    * Error running query on master: %-.128s
    */
   const val ER_QUERY_ON_MASTER: Int = 1219

   /**
    * Error when executing command %s: %-.128s
    */
   const val ER_ERROR_WHEN_EXECUTING_COMMAND: Int = 1220

   /**
    * Incorrect usage of %s and %s
    */
   const val ER_WRONG_USAGE: Int = 1221

   /**
    * The used SELECT statements have a different number of columns
    */
   const val ER_WRONG_NUMBER_OF_COLUMNS_IN_SELECT: Int = 1222

   /**
    * Can't execute the query because you have a conflicting read lock
    */
   const val ER_CANT_UPDATE_WITH_READLOCK: Int = 1223

   /**
    * Mixing of transactional and non-transactional tables is disabled
    */
   const val ER_MIXING_NOT_ALLOWED: Int = 1224

   /**
    * Option '%s' used twice in statement
    */
   const val ER_DUP_ARGUMENT: Int = 1225

   /**
    * User '%-.64s' has exceeded the '%s' resource (current value: %ld)
    */
   const val ER_USER_LIMIT_REACHED: Int = 1226

   /**
    * Access denied; you need (at least one of) the %-.128s privilege(s) for this operation
    */
   const val ER_SPECIFIC_ACCESS_DENIED_ERROR: Int = 1227

   /**
    * Variable '%-.64s' is a SESSION variable and can't be used with SET GLOBAL
    */
   const val ER_LOCAL_VARIABLE: Int = 1228

   /**
    * Variable '%-.64s' is a GLOBAL variable and should be set with SET GLOBAL
    */
   const val ER_GLOBAL_VARIABLE: Int = 1229

   /**
    * Variable '%-.64s' doesn't have a default value
    */
   const val ER_NO_DEFAULT: Int = 1230

   /**
    * Variable '%-.64s' can't be set to the value of '%-.200s'
    */
   const val ER_WRONG_VALUE_FOR_VAR: Int = 1231

   /**
    * Incorrect argument type to variable '%-.64s'
    */
   const val ER_WRONG_TYPE_FOR_VAR: Int = 1232

   /**
    * Variable '%-.64s' can only be set, not read
    */
   const val ER_VAR_CANT_BE_READ: Int = 1233

   /**
    * Incorrect usage/placement of '%s'
    */
   const val ER_CANT_USE_OPTION_HERE: Int = 1234

   /**
    * This version of MySQL doesn't yet support '%s'
    */
   const val ER_NOT_SUPPORTED_YET: Int = 1235

   /**
    * Got fatal error %d from master when reading data from binary log: '%-.320s'
    */
   const val ER_MASTER_FATAL_ERROR_READING_BINLOG: Int = 1236

   /**
    * Slave SQL thread ignored the query because of replicate-*-table rules
    */
   const val ER_SLAVE_IGNORED_TABLE: Int = 1237

   /**
    * Variable '%-.192s' is a %s variable
    */
   const val ER_INCORRECT_GLOBAL_LOCAL_VAR: Int = 1238

   /**
    * Incorrect foreign key definition for '%-.192s': %s
    */
   const val ER_WRONG_FK_DEF: Int = 1239

   /**
    * Key reference and table reference don't match
    */
   const val ER_KEY_REF_DO_NOT_MATCH_TABLE_REF: Int = 1240

   /**
    * Operand should contain %d column(s)
    */
   const val ER_OPERAND_COLUMNS: Int = 1241

   /**
    * Subquery returns more than 1 row
    */
   const val ER_SUBQUERY_NO_1_ROW: Int = 1242

   /**
    * Unknown prepared statement handler (%.*s) given to %s
    */
   const val ER_UNKNOWN_STMT_HANDLER: Int = 1243

   /**
    * Help database is corrupt or does not exist
    */
   const val ER_CORRUPT_HELP_DB: Int = 1244

   /**
    * Cyclic reference on subqueries
    */
   const val ER_CYCLIC_REFERENCE: Int = 1245

   /**
    * Converting column '%s' from %s to %s
    */
   const val ER_AUTO_CONVERT: Int = 1246

   /**
    * Reference '%-.64s' not supported (%s)
    */
   const val ER_ILLEGAL_REFERENCE: Int = 1247

   /**
    * Every derived table must have its own alias
    */
   const val ER_DERIVED_MUST_HAVE_ALIAS: Int = 1248

   /**
    * Select %u was reduced during optimization
    */
   const val ER_SELECT_REDUCED: Int = 1249

   /**
    * Table '%-.192s' from one of the SELECTs cannot be used in %-.32s
    */
   const val ER_TABLENAME_NOT_ALLOWED_HERE: Int = 1250

   /**
    * Client does not support authentication protocol requested by server; consider upgrading MySQL client
    */
   const val ER_NOT_SUPPORTED_AUTH_MODE: Int = 1251

   /**
    * All parts of a SPATIAL index must be NOT NULL
    */
   const val ER_SPATIAL_CANT_HAVE_NULL: Int = 1252

   /**
    * COLLATION '%s' is not valid for CHARACTER SET '%s'
    */
   const val ER_COLLATION_CHARSET_MISMATCH: Int = 1253

   /**
    * Slave is already running
    */
   const val ER_SLAVE_WAS_RUNNING: Int = 1254

   /**
    * Slave already has been stopped
    */
   const val ER_SLAVE_WAS_NOT_RUNNING: Int = 1255

   /**
    * Uncompressed data size too large; the maximum size is %d (probably, length of uncompressed data was corrupted)
    */
   const val ER_TOO_BIG_FOR_UNCOMPRESS: Int = 1256

   /**
    * ZLIB: Not enough memory
    */
   const val ER_ZLIB_Z_MEM_ERROR: Int = 1257

   /**
    * ZLIB: Not enough room in the output buffer (probably, length of uncompressed data was corrupted)
    */
   const val ER_ZLIB_Z_BUF_ERROR: Int = 1258

   /**
    * ZLIB: Input data corrupted
    */
   const val ER_ZLIB_Z_DATA_ERROR: Int = 1259

   /**
    * Row %u was cut by GROUP_CONCAT()
    */
   const val ER_CUT_VALUE_GROUP_CONCAT: Int = 1260

   /**
    * Row %ld doesn't contain data for all columns
    */
   const val ER_WARN_TOO_FEW_RECORDS: Int = 1261

   /**
    * Row %ld was truncated; it contained more data than there were input columns
    */
   const val ER_WARN_TOO_MANY_RECORDS: Int = 1262

   /**
    * Column set to default value; NULL supplied to NOT NULL column '%s' at row %ld
    */
   const val ER_WARN_NULL_TO_NOTNULL: Int = 1263

   /**
    * Out of range value for column '%s' at row %ld
    */
   const val ER_WARN_DATA_OUT_OF_RANGE: Int = 1264

   /**
    * Data truncated for column '%s' at row %ld
    */
   const val WARN_DATA_TRUNCATED: Int = 1265

   /**
    * Using storage engine %s for table '%s'
    */
   const val ER_WARN_USING_OTHER_HANDLER: Int = 1266

   /**
    * Illegal mix of collations (%s,%s) and (%s,%s) for operation '%s'
    */
   const val ER_CANT_AGGREGATE_2COLLATIONS: Int = 1267

   /**
    * Cannot drop one or more of the requested users
    */
   const val ER_DROP_USER: Int = 1268

   /**
    * Can't revoke all privileges for one or more of the requested users
    */
   const val ER_REVOKE_GRANTS: Int = 1269

   /**
    * Illegal mix of collations (%s,%s), (%s,%s), (%s,%s) for operation '%s'
    */
   const val ER_CANT_AGGREGATE_3COLLATIONS: Int = 1270

   /**
    * Illegal mix of collations for operation '%s'
    */
   const val ER_CANT_AGGREGATE_NCOLLATIONS: Int = 1271

   /**
    * Variable '%-.64s' is not a variable component (can't be used as XXXX.variable_name)
    */
   const val ER_VARIABLE_IS_NOT_STRUCT: Int = 1272

   /**
    * Unknown collation: '%-.64s'
    */
   const val ER_UNKNOWN_COLLATION: Int = 1273

   /**
    * SSL parameters in CHANGE MASTER are ignored because this MySQL slave was compiled without SSL support; they
    * can be used later if MySQL slave with SSL is started
    */
   const val ER_SLAVE_IGNORED_SSL_PARAMS: Int = 1274

   /**
    * Server is running in --secure-auth mode, but '%s'@'%s' has a password in the old format; please change the
    * password to the new format
    */
   const val ER_SERVER_IS_IN_SECURE_AUTH_MODE: Int = 1275

   /**
    * Field or reference '%-.192s%s%-.192s%s%-.192s' of SELECT #%d was resolved in SELECT #%d
    */
   const val ER_WARN_FIELD_RESOLVED: Int = 1276

   /**
    * Incorrect parameter or combination of parameters for START SLAVE UNTIL
    */
   const val ER_BAD_SLAVE_UNTIL_COND: Int = 1277

   /**
    * It is recommended to use --skip-slave-start when doing step-by-step replication with START SLAVE UNTIL;
    * otherwise, you will get problems if you get an unexpected slave's mysqld restart
    */
   const val ER_MISSING_SKIP_SLAVE: Int = 1278

   /**
    * SQL thread is not to be started so UNTIL options are ignored
    */
   const val ER_UNTIL_COND_IGNORED: Int = 1279

   /**
    * Incorrect index name '%-.100s'
    */
   const val ER_WRONG_NAME_FOR_INDEX: Int = 1280

   /**
    * Incorrect catalog name '%-.100s'
    */
   const val ER_WRONG_NAME_FOR_CATALOG: Int = 1281

   /**
    * Query cache failed to set size %lu; new query cache size is %lu
    */
   const val ER_WARN_QC_RESIZE: Int = 1282

   /**
    * Column '%-.192s' cannot be part of FULLTEXT index
    */
   const val ER_BAD_FT_COLUMN: Int = 1283

   /**
    * Unknown key cache '%-.100s'
    */
   const val ER_UNKNOWN_KEY_CACHE: Int = 1284

   /**
    * MySQL is started in --skip-name-resolve mode; you must restart it without this switch for this grant to work
    */
   const val ER_WARN_HOSTNAME_WONT_WORK: Int = 1285

   /**
    * Unknown storage engine '%s'
    */
   const val ER_UNKNOWN_STORAGE_ENGINE: Int = 1286

   /**
    * '%s' is deprecated and will be removed in a future release. Please use %s instead
    */
   const val ER_WARN_DEPRECATED_SYNTAX: Int = 1287

   /**
    * The target table %-.100s of the %s is not updatable
    */
   const val ER_NON_UPDATABLE_TABLE: Int = 1288

   /**
    * The '%s' feature is disabled; you need MySQL built with '%s' to have it working
    */
   const val ER_FEATURE_DISABLED: Int = 1289

   /**
    * The MySQL server is running with the %s option so it cannot execute this statement
    */
   const val ER_OPTION_PREVENTS_STATEMENT: Int = 1290

   /**
    * Column '%-.100s' has duplicated value '%-.64s' in %s
    */
   const val ER_DUPLICATED_VALUE_IN_TYPE: Int = 1291

   /**
    * Truncated incorrect %-.32s value: '%-.128s'
    */
   const val ER_TRUNCATED_WRONG_VALUE: Int = 1292

   /**
    * Incorrect table definition; there can be only one TIMESTAMP column with CURRENT_TIMESTAMP in DEFAULT or ON
    * UPDATE clause
    */
   const val ER_TOO_MUCH_AUTO_TIMESTAMP_COLS: Int = 1293

   /**
    * Invalid ON UPDATE clause for '%-.192s' column
    */
   const val ER_INVALID_ON_UPDATE: Int = 1294

   /**
    * This command is not supported in the prepared statement protocol yet
    */
   const val ER_UNSUPPORTED_PS: Int = 1295

   /**
    * Got error %d '%-.100s' from %s
    */
   const val ER_GET_ERRMSG: Int = 1296

   /**
    * Got temporary error %d '%-.100s' from %s
    */
   const val ER_GET_TEMPORARY_ERRMSG: Int = 1297

   /**
    * Unknown or incorrect time zone: '%-.64s'
    */
   const val ER_UNKNOWN_TIME_ZONE: Int = 1298

   /**
    * Invalid TIMESTAMP value in column '%s' at row %ld
    */
   const val ER_WARN_INVALID_TIMESTAMP: Int = 1299

   /**
    * Invalid %s character string: '%.64s'
    */
   const val ER_INVALID_CHARACTER_STRING: Int = 1300

   /**
    * Result of %s() was larger than max_allowed_packet (%ld) - truncated
    */
   const val ER_WARN_ALLOWED_PACKET_OVERFLOWED: Int = 1301

   /**
    * Conflicting declarations: '%s%s' and '%s%s'
    */
   const val ER_CONFLICTING_DECLARATIONS: Int = 1302

   /**
    * Can't create a %s from within another stored routine
    */
   const val ER_SP_NO_RECURSIVE_CREATE: Int = 1303

   /**
    * %s %s already exists
    */
   const val ER_SP_ALREADY_EXISTS: Int = 1304

   /**
    * %s %s does not exist
    */
   const val ER_SP_DOES_NOT_EXIST: Int = 1305

   /**
    * Failed to DROP %s %s
    */
   const val ER_SP_DROP_FAILED: Int = 1306

   /**
    * Failed to CREATE %s %s
    */
   const val ER_SP_STORE_FAILED: Int = 1307

   /**
    * %s with no matching label: %s
    */
   const val ER_SP_LILABEL_MISMATCH: Int = 1308

   /**
    * Redefining label %s
    */
   const val ER_SP_LABEL_REDEFINE: Int = 1309

   /**
    * End-label %s without match
    */
   const val ER_SP_LABEL_MISMATCH: Int = 1310

   /**
    * Referring to uninitialized variable %s
    */
   const val ER_SP_UNINIT_VAR: Int = 1311

   /**
    * PROCEDURE %s can't return a result set in the given context
    */
   const val ER_SP_BADSELECT: Int = 1312

   /**
    * RETURN is only allowed in a FUNCTION
    */
   const val ER_SP_BADRETURN: Int = 1313

   /**
    * %s is not allowed in stored procedures
    */
   const val ER_SP_BADSTATEMENT: Int = 1314

   /**
    * The update log is deprecated and replaced by the binary log; SET SQL_LOG_UPDATE has been ignored.
    */
   const val ER_UPDATE_LOG_DEPRECATED_IGNORED: Int = 1315

   /**
    * The update log is deprecated and replaced by the binary log; SET SQL_LOG_UPDATE has been translated to SET
    * SQL_LOG_BIN.
    */
   const val ER_UPDATE_LOG_DEPRECATED_TRANSLATED: Int = 1316

   /**
    * Query execution was interrupted
    */
   const val ER_QUERY_INTERRUPTED: Int = 1317

   /**
    * Incorrect number of arguments for %s %s; expected %u, got %u
    */
   const val ER_SP_WRONG_NO_OF_ARGS: Int = 1318

   /**
    * Undefined CONDITION: %s
    */
   const val ER_SP_COND_MISMATCH: Int = 1319

   /**
    * No RETURN found in FUNCTION %s
    */
   const val ER_SP_NORETURN: Int = 1320

   /**
    * FUNCTION %s ended without RETURN
    */
   const val ER_SP_NORETURNEND: Int = 1321

   /**
    * Cursor statement must be a SELECT
    */
   const val ER_SP_BAD_CURSOR_QUERY: Int = 1322

   /**
    * Cursor SELECT must not have INTO
    */
   const val ER_SP_BAD_CURSOR_SELECT: Int = 1323

   /**
    * Undefined CURSOR: %s
    */
   const val ER_SP_CURSOR_MISMATCH: Int = 1324

   /**
    * Cursor is already open
    */
   const val ER_SP_CURSOR_ALREADY_OPEN: Int = 1325

   /**
    * Cursor is not open
    */
   const val ER_SP_CURSOR_NOT_OPEN: Int = 1326

   /**
    * Undeclared variable: %s
    */
   const val ER_SP_UNDECLARED_VAR: Int = 1327

   /**
    * Incorrect number of FETCH variables
    */
   const val ER_SP_WRONG_NO_OF_FETCH_ARGS: Int = 1328

   /**
    * No data - zero rows fetched, selected, or processed
    */
   const val ER_SP_FETCH_NO_DATA: Int = 1329

   /**
    * Duplicate parameter: %s
    */
   const val ER_SP_DUP_PARAM: Int = 1330

   /**
    * Duplicate variable: %s
    */
   const val ER_SP_DUP_VAR: Int = 1331

   /**
    * Duplicate condition: %s
    */
   const val ER_SP_DUP_COND: Int = 1332

   /**
    * Duplicate cursor: %s
    */
   const val ER_SP_DUP_CURS: Int = 1333

   /**
    * Failed to ALTER %s %s
    */
   const val ER_SP_CANT_ALTER: Int = 1334

   /**
    * Subquery value not supported
    */
   const val ER_SP_SUBSELECT_NYI: Int = 1335

   /**
    * %s is not allowed in stored function or trigger
    */
   const val ER_STMT_NOT_ALLOWED_IN_SF_OR_TRG: Int = 1336

   /**
    * Variable or condition declaration after cursor or handler declaration
    */
   const val ER_SP_VARCOND_AFTER_CURSHNDLR: Int = 1337

   /**
    * Cursor declaration after handler declaration
    */
   const val ER_SP_CURSOR_AFTER_HANDLER: Int = 1338

   /**
    * Case not found for CASE statement
    */
   const val ER_SP_CASE_NOT_FOUND: Int = 1339

   /**
    * Configuration file '%-.192s' is too big
    */
   const val ER_FPARSER_TOO_BIG_FILE: Int = 1340

   /**
    * Malformed file type header in file '%-.192s'
    */
   const val ER_FPARSER_BAD_HEADER: Int = 1341

   /**
    * Unexpected end of file while parsing comment '%-.200s'
    */
   const val ER_FPARSER_EOF_IN_COMMENT: Int = 1342

   /**
    * Error while parsing parameter '%-.192s' (line: '%-.192s')
    */
   const val ER_FPARSER_ERROR_IN_PARAMETER: Int = 1343

   /**
    * Unexpected end of file while skipping unknown parameter '%-.192s'
    */
   const val ER_FPARSER_EOF_IN_UNKNOWN_PARAMETER: Int = 1344

   /**
    * EXPLAIN/SHOW can not be issued; lacking privileges for underlying table
    */
   const val ER_VIEW_NO_EXPLAIN: Int = 1345

   /**
    * File '%-.192s' has unknown type '%-.64s' in its header
    */
   const val ER_FRM_UNKNOWN_TYPE: Int = 1346

   /**
    * '%-.192s.%-.192s' is not %s
    */
   const val ER_WRONG_OBJECT: Int = 1347

   /**
    * Column '%-.192s' is not updatable
    */
   const val ER_NONUPDATEABLE_COLUMN: Int = 1348

   /**
    * View's SELECT contains a subquery in the FROM clause
    */
   const val ER_VIEW_SELECT_DERIVED: Int = 1349

   /**
    * View's SELECT contains a '%s' clause
    */
   const val ER_VIEW_SELECT_CLAUSE: Int = 1350

   /**
    * View's SELECT contains a variable or parameter
    */
   const val ER_VIEW_SELECT_VARIABLE: Int = 1351

   /**
    * View's SELECT refers to a temporary table '%-.192s'
    */
   const val ER_VIEW_SELECT_TMPTABLE: Int = 1352

   /**
    * View's SELECT and view's field list have different column counts
    */
   const val ER_VIEW_WRONG_LIST: Int = 1353

   /**
    * View merge algorithm can't be used here for now (assumed undefined algorithm)
    */
   const val ER_WARN_VIEW_MERGE: Int = 1354

   /**
    * View being updated does not have complete key of underlying table in it
    */
   const val ER_WARN_VIEW_WITHOUT_KEY: Int = 1355

   /**
    * View '%-.192s.%-.192s' references invalid table(s) or column(s) or function(s) or definer/invoker of view lack
    * rights to use them
    */
   const val ER_VIEW_INVALID: Int = 1356

   /**
    * Can't drop or alter a %s from within another stored routine
    */
   const val ER_SP_NO_DROP_SP: Int = 1357

   /**
    * GOTO is not allowed in a stored procedure handler
    */
   const val ER_SP_GOTO_IN_HNDLR: Int = 1358

   /**
    * Trigger already exists
    */
   const val ER_TRG_ALREADY_EXISTS: Int = 1359

   /**
    * Trigger does not exist
    */
   const val ER_TRG_DOES_NOT_EXIST: Int = 1360

   /**
    * Trigger's '%-.192s' is view or temporary table
    */
   const val ER_TRG_ON_VIEW_OR_TEMP_TABLE: Int = 1361

   /**
    * Updating of %s row is not allowed in %strigger
    */
   const val ER_TRG_CANT_CHANGE_ROW: Int = 1362

   /**
    * There is no %s row in %s trigger
    */
   const val ER_TRG_NO_SUCH_ROW_IN_TRG: Int = 1363

   /**
    * Field '%-.192s' doesn't have a default value
    */
   const val ER_NO_DEFAULT_FOR_FIELD: Int = 1364

   /**
    * Division by 0
    */
   const val ER_DIVISION_BY_ZERO: Int = 1365

   /**
    * Incorrect %-.32s value: '%-.128s' for column '%.192s' at row %ld
    */
   const val ER_TRUNCATED_WRONG_VALUE_FOR_FIELD: Int = 1366

   /**
    * Illegal %s '%-.192s' value found during parsing
    */
   const val ER_ILLEGAL_VALUE_FOR_TYPE: Int = 1367

   /**
    * CHECK OPTION on non-updatable view '%-.192s.%-.192s'
    */
   const val ER_VIEW_NONUPD_CHECK: Int = 1368

   /**
    * CHECK OPTION failed '%-.192s.%-.192s'
    */
   const val ER_VIEW_CHECK_FAILED: Int = 1369

   /**
    * %-.16s command denied to user '%-.48s'@'%-.64s' for routine '%-.192s'
    */
   const val ER_PROCACCESS_DENIED_ERROR: Int = 1370

   /**
    * Failed purging old relay logs: %s
    */
   const val ER_RELAY_LOG_FAIL: Int = 1371

   /**
    * Password hash should be a %d-digit hexadecimal number
    */
   const val ER_PASSWD_LENGTH: Int = 1372

   /**
    * Target log not found in binlog index
    */
   const val ER_UNKNOWN_TARGET_BINLOG: Int = 1373

   /**
    * I/O error reading log index file
    */
   const val ER_IO_ERR_LOG_INDEX_READ: Int = 1374

   /**
    * Server configuration does not permit binlog purge
    */
   const val ER_BINLOG_PURGE_PROHIBITED: Int = 1375

   /**
    * Failed on fseek()
    */
   const val ER_FSEEK_FAIL: Int = 1376

   /**
    * Fatal error during log purge
    */
   const val ER_BINLOG_PURGE_FATAL_ERR: Int = 1377

   /**
    * A purgeable log is in use, will not purge
    */
   const val ER_LOG_IN_USE: Int = 1378

   /**
    * Unknown error during log purge
    */
   const val ER_LOG_PURGE_UNKNOWN_ERR: Int = 1379

   /**
    * Failed initializing relay log position: %s
    */
   const val ER_RELAY_LOG_INIT: Int = 1380

   /**
    * You are not using binary logging
    */
   const val ER_NO_BINARY_LOGGING: Int = 1381

   /**
    * The '%-.64s' syntax is reserved for purposes internal to the MySQL server
    */
   const val ER_RESERVED_SYNTAX: Int = 1382

   /**
    * WSAStartup Failed
    */
   const val ER_WSAS_FAILED: Int = 1383

   /**
    * Can't handle procedures with different groups yet
    */
   const val ER_DIFF_GROUPS_PROC: Int = 1384

   /**
    * Select must have a group with this procedure
    */
   const val ER_NO_GROUP_FOR_PROC: Int = 1385

   /**
    * Can't use ORDER clause with this procedure
    */
   const val ER_ORDER_WITH_PROC: Int = 1386

   /**
    * Binary logging and replication forbid changing the global server %s
    */
   const val ER_LOGGING_PROHIBIT_CHANGING_OF: Int = 1387

   /**
    * Can't map file: %-.200s, errno: %d
    */
   const val ER_NO_FILE_MAPPING: Int = 1388

   /**
    * Wrong magic in %-.64s
    */
   const val ER_WRONG_MAGIC: Int = 1389

   /**
    * Prepared statement contains too many placeholders
    */
   const val ER_PS_MANY_PARAM: Int = 1390

   /**
    * Key part '%-.192s' length cannot be 0
    */
   const val ER_KEY_PART_0: Int = 1391

   /**
    * View text checksum failed
    */
   const val ER_VIEW_CHECKSUM: Int = 1392

   /**
    * Can not modify more than one base table through a join view '%-.192s.%-.192s'
    */
   const val ER_VIEW_MULTIUPDATE: Int = 1393

   /**
    * Can not insert into join view '%-.192s.%-.192s' without fields list
    */
   const val ER_VIEW_NO_INSERT_FIELD_LIST: Int = 1394

   /**
    * Can not delete from join view '%-.192s.%-.192s'
    */
   const val ER_VIEW_DELETE_MERGE_VIEW: Int = 1395

   /**
    * Operation %s failed for %.256s
    */
   const val ER_CANNOT_USER: Int = 1396

   /**
    * XAER_NOTA: Unknown XID
    */
   const val ER_XAER_NOTA: Int = 1397

   /**
    * XAER_INVAL: Invalid arguments (or unsupported command)
    */
   const val ER_XAER_INVAL: Int = 1398

   /**
    * XAER_RMFAIL: The command cannot be executed when global transaction is in the  %.64s state
    */
   const val ER_XAER_RMFAIL: Int = 1399

   /**
    * XAER_OUTSIDE: Some work is done outside global transaction
    */
   const val ER_XAER_OUTSIDE: Int = 1400

   /**
    * XAER_RMERR: Fatal error occurred in the transaction branch - check your data for consistency
    */
   const val ER_XAER_RMERR: Int = 1401

   /**
    * XA_RBROLLBACK: Transaction branch was rolled back
    */
   const val ER_XA_RBROLLBACK: Int = 1402

   /**
    * There is no such grant defined for user '%-.48s' on host '%-.64s' on routine '%-.192s'
    */
   const val ER_NONEXISTING_PROC_GRANT: Int = 1403

   /**
    * Failed to grant EXECUTE and ALTER ROUTINE privileges
    */
   const val ER_PROC_AUTO_GRANT_FAIL: Int = 1404

   /**
    * Failed to revoke all privileges to dropped routine
    */
   const val ER_PROC_AUTO_REVOKE_FAIL: Int = 1405

   /**
    * Data too long for column '%s' at row %ld
    */
   const val ER_DATA_TOO_LONG: Int = 1406

   /**
    * Bad SQLSTATE: '%s'
    */
   const val ER_SP_BAD_SQLSTATE: Int = 1407

   /**
    * %s: ready for connections.\nVersion: '%s'  socket: '%s'  port: %d  %s
    */
   const val ER_STARTUP: Int = 1408

   /**
    * Can't load value from file with fixed size rows to variable
    */
   const val ER_LOAD_FROM_FIXED_SIZE_ROWS_TO_VAR: Int = 1409

   /**
    * You are not allowed to create a user with GRANT
    */
   const val ER_CANT_CREATE_USER_WITH_GRANT: Int = 1410

   /**
    * Incorrect %-.32s value: '%-.128s' for function %-.32s
    */
   const val ER_WRONG_VALUE_FOR_TYPE: Int = 1411

   /**
    * Table definition has changed, please retry transaction
    */
   const val ER_TABLE_DEF_CHANGED: Int = 1412

   /**
    * Duplicate handler declared in the same block
    */
   const val ER_SP_DUP_HANDLER: Int = 1413

   /**
    * OUT or INOUT argument %d for routine %s is not a variable or NEW pseudo-variable in BEFORE trigger
    */
   const val ER_SP_NOT_VAR_ARG: Int = 1414

   /**
    * Not allowed to return a result set from a %s
    */
   const val ER_SP_NO_RETSET: Int = 1415

   /**
    * Cannot get geometry object from data you send to the GEOMETRY field
    */
   const val ER_CANT_CREATE_GEOMETRY_OBJECT: Int = 1416

   /**
    * A routine failed and has neither NO SQL nor READS SQL DATA in its declaration and binary logging is enabled;
    * if non-transactional tables were updated, the binary log will miss their changes
    */
   const val ER_FAILED_ROUTINE_BREAK_BINLOG: Int = 1417

   /**
    * This function has none of DETERMINISTIC, NO SQL, or READS SQL DATA in its declaration and binary logging is
    * enabled (you *might* want to use the less safe log_bin_trust_function_creators variable)
    */
   const val ER_BINLOG_UNSAFE_ROUTINE: Int = 1418

   /**
    * You do not have the SUPER privilege and binary logging is enabled (you *might* want to use the less safe
    * log_bin_trust_function_creators variable)
    */
   const val ER_BINLOG_CREATE_ROUTINE_NEED_SUPER: Int = 1419

   /**
    * You can't execute a prepared statement which has an open cursor associated with it. Reset the statement to
    * re-execute it.
    */
   const val ER_EXEC_STMT_WITH_OPEN_CURSOR: Int = 1420

   /**
    * The statement (%lu) has no open cursor.
    */
   const val ER_STMT_HAS_NO_OPEN_CURSOR: Int = 1421

   /**
    * Explicit or implicit commit is not allowed in stored function or trigger.
    */
   const val ER_COMMIT_NOT_ALLOWED_IN_SF_OR_TRG: Int = 1422

   /**
    * Field of view '%-.192s.%-.192s' underlying table doesn't have a default value
    */
   const val ER_NO_DEFAULT_FOR_VIEW_FIELD: Int = 1423

   /**
    * Recursive stored functions and triggers are not allowed.
    */
   const val ER_SP_NO_RECURSION: Int = 1424

   /**
    * Too big scale %d specified for column '%-.192s'. Maximum is %lu.
    */
   const val ER_TOO_BIG_SCALE: Int = 1425

   /**
    * Too big precision %d specified for column '%-.192s'. Maximum is %lu.
    */
   const val ER_TOO_BIG_PRECISION: Int = 1426

   /**
    * For float(M,D), double(M,D) or decimal(M,D), M must be &gt;= D (column '%-.192s').
    */
   const val ER_M_BIGGER_THAN_D: Int = 1427

   /**
    * You can't combine write-locking of system tables with other tables or lock types
    */
   const val ER_WRONG_LOCK_OF_SYSTEM_TABLE: Int = 1428

   /**
    * Unable to connect to foreign data source: %.64s
    */
   const val ER_CONNECT_TO_FOREIGN_DATA_SOURCE: Int = 1429

   /**
    * There was a problem processing the query on the foreign data source. Data source error: %-.64s
    */
   const val ER_QUERY_ON_FOREIGN_DATA_SOURCE: Int = 1430

   /**
    * The foreign data source you are trying to reference does not exist. Data source error:  %-.64s
    */
   const val ER_FOREIGN_DATA_SOURCE_DOESNT_EXIST: Int = 1431

   /**
    * Can't create federated table. The data source connection string '%-.64s' is not in the correct format
    */
   const val ER_FOREIGN_DATA_STRING_INVALID_CANT_CREATE: Int = 1432

   /**
    * The data source connection string '%-.64s' is not in the correct format
    */
   const val ER_FOREIGN_DATA_STRING_INVALID: Int = 1433

   /**
    * Can't create federated table. Foreign data src error:  %-.64s
    */
   const val ER_CANT_CREATE_FEDERATED_TABLE: Int = 1434

   /**
    * Trigger in wrong schema
    */
   const val ER_TRG_IN_WRONG_SCHEMA: Int = 1435

   /**
    * Thread stack overrun:  %ld bytes used of a %ld byte stack, and %ld bytes needed.  Use 'mysqld
    * --thread_stack=#' to specify a bigger stack.
    */
   const val ER_STACK_OVERRUN_NEED_MORE: Int = 1436

   /**
    * Routine body for '%-.100s' is too long
    */
   const val ER_TOO_LONG_BODY: Int = 1437

   /**
    * Cannot drop default keycache
    */
   const val ER_WARN_CANT_DROP_DEFAULT_KEYCACHE: Int = 1438

   /**
    * Display width out of range for column '%-.192s' (max = %lu)
    */
   const val ER_TOO_BIG_DISPLAYWIDTH: Int = 1439

   /**
    * XAER_DUPID: The XID already exists
    */
   const val ER_XAER_DUPID: Int = 1440

   /**
    * Datetime function: %-.32s field overflow
    */
   const val ER_DATETIME_FUNCTION_OVERFLOW: Int = 1441

   /**
    * Can't update table '%-.192s' in stored function/trigger because it is already used by statement which invoked
    * this stored function/trigger.
    */
   const val ER_CANT_UPDATE_USED_TABLE_IN_SF_OR_TRG: Int = 1442

   /**
    * The definition of table '%-.192s' prevents operation %.192s on table '%-.192s'.
    */
   const val ER_VIEW_PREVENT_UPDATE: Int = 1443

   /**
    * The prepared statement contains a stored routine call that refers to that same statement. It's not allowed to
    * execute a prepared statement in such a recursive manner
    */
   const val ER_PS_NO_RECURSION: Int = 1444

   /**
    * Not allowed to set autocommit from a stored function or trigger
    */
   const val ER_SP_CANT_SET_AUTOCOMMIT: Int = 1445

   /**
    * Definer is not fully qualified
    */
   const val ER_MALFORMED_DEFINER: Int = 1446

   /**
    * View '%-.192s'.'%-.192s' has no definer information (old table format). Current user is used as definer.
    * Please recreate the view!
    */
   const val ER_VIEW_FRM_NO_USER: Int = 1447

   /**
    * You need the SUPER privilege for creation view with '%-.192s'@'%-.192s' definer
    */
   const val ER_VIEW_OTHER_USER: Int = 1448

   /**
    * The user specified as a definer ('%-.64s'@'%-.64s') does not exist
    */
   const val ER_NO_SUCH_USER: Int = 1449

   /**
    * Changing schema from '%-.192s' to '%-.192s' is not allowed.
    */
   const val ER_FORBID_SCHEMA_CHANGE: Int = 1450

   /**
    * Cannot delete or update a parent row: a foreign key constraint fails (%.192s)
    */
   const val ER_ROW_IS_REFERENCED_2: Int = 1451

   /**
    * Cannot add or update a child row: a foreign key constraint fails (%.192s)
    */
   const val ER_NO_REFERENCED_ROW_2: Int = 1452

   /**
    * Variable '%-.64s' must be quoted with `...`, or renamed
    */
   const val ER_SP_BAD_VAR_SHADOW: Int = 1453

   /**
    * No definer attribute for trigger '%-.192s'.'%-.192s'. The trigger will be activated under the authorization of
    * the invoker, which may have insufficient privileges. Please recreate the trigger.
    */
   const val ER_TRG_NO_DEFINER: Int = 1454

   /**
    * '%-.192s' has an old format, you should re-create the '%s' object(s)
    */
   const val ER_OLD_FILE_FORMAT: Int = 1455

   /**
    * Recursive limit %d (as set by the max_sp_recursion_depth variable) was exceeded for routine %.192s
    */
   const val ER_SP_RECURSION_LIMIT: Int = 1456

   /**
    * Failed to load routine %-.192s. The table mysql.proc is missing, corrupt, or contains bad data (internal code
    * %d)
    */
   const val ER_SP_PROC_TABLE_CORRUPT: Int = 1457

   /**
    * Incorrect routine name '%-.192s'
    */
   const val ER_SP_WRONG_NAME: Int = 1458

   /**
    * Table upgrade required. Please do \"REPAIR TABLE `%-.32s`\" or dump/reload to fix it!
    */
   const val ER_TABLE_NEEDS_UPGRADE: Int = 1459

   /**
    * AGGREGATE is not supported for stored functions
    */
   const val ER_SP_NO_AGGREGATE: Int = 1460

   /**
    * Can't create more than max_prepared_stmt_count statements (current value: %lu)
    */
   const val ER_MAX_PREPARED_STMT_COUNT_REACHED: Int = 1461

   /**
    * `%-.192s`.`%-.192s` contains view recursion
    */
   const val ER_VIEW_RECURSIVE: Int = 1462

   /**
    * Non-grouping field '%-.192s' is used in %-.64s clause
    */
   const val ER_NON_GROUPING_FIELD_USED: Int = 1463

   /**
    * The used table type doesn't support SPATIAL indexes
    */
   const val ER_TABLE_CANT_HANDLE_SPKEYS: Int = 1464

   /**
    * Triggers can not be created on system tables
    */
   const val ER_NO_TRIGGERS_ON_SYSTEM_SCHEMA: Int = 1465

   /**
    * Leading spaces are removed from name '%s'
    */
   const val ER_REMOVED_SPACES: Int = 1466

   /**
    * Failed to read auto-increment value from storage engine
    */
   const val ER_AUTOINC_READ_FAILED: Int = 1467

   /**
    * User name
    */
   const val ER_USERNAME: Int = 1468

   /**
    * Host name
    */
   const val ER_HOSTNAME: Int = 1469

   /**
    * String '%-.70s' is too long for %s (should be no longer than %d)
    */
   const val ER_WRONG_STRING_LENGTH: Int = 1470

   /**
    * The target table %-.100s of the %s is not insertable-into
    */
   const val ER_NON_INSERTABLE_TABLE: Int = 1471

   /**
    * Table '%-.64s' is differently defined or of non-MyISAM type or doesn't exist
    */
   const val ER_ADMIN_WRONG_MRG_TABLE: Int = 1472

   /**
    * Too high level of nesting for select
    */
   const val ER_TOO_HIGH_LEVEL_OF_NESTING_FOR_SELECT: Int = 1473

   /**
    * Name '%-.64s' has become ''
    */
   const val ER_NAME_BECOMES_EMPTY: Int = 1474

   /**
    * First character of the FIELDS TERMINATED string is ambiguous; please use non-optional and non-empty FIELDS
    * ENCLOSED BY
    */
   const val ER_AMBIGUOUS_FIELD_TERM: Int = 1475

   /**
    * The foreign server, %s, you are trying to create already exists.
    */
   const val ER_FOREIGN_SERVER_EXISTS: Int = 1476

   /**
    * The foreign server name you are trying to reference does not exist. Data source error:  %-.64s
    */
   const val ER_FOREIGN_SERVER_DOESNT_EXIST: Int = 1477

   /**
    * Table storage engine '%-.64s' does not support the create option '%.64s'
    */
   const val ER_ILLEGAL_HA_CREATE_OPTION: Int = 1478

   /**
    * Syntax error: %-.64s PARTITIONING requires definition of VALUES %-.64s for each partition
    */
   const val ER_PARTITION_REQUIRES_VALUES_ERROR: Int = 1479

   /**
    * Only %-.64s PARTITIONING can use VALUES %-.64s in partition definition
    */
   const val ER_PARTITION_WRONG_VALUES_ERROR: Int = 1480

   /**
    * MAXVALUE can only be used in last partition definition
    */
   const val ER_PARTITION_MAXVALUE_ERROR: Int = 1481

   /**
    * Subpartitions can only be hash partitions and by key
    */
   const val ER_PARTITION_SUBPARTITION_ERROR: Int = 1482

   /**
    * Must define subpartitions on all partitions if on one partition
    */
   const val ER_PARTITION_SUBPART_MIX_ERROR: Int = 1483

   /**
    * Wrong number of partitions defined, mismatch with previous setting
    */
   const val ER_PARTITION_WRONG_NO_PART_ERROR: Int = 1484

   /**
    * Wrong number of subpartitions defined, mismatch with previous setting
    */
   const val ER_PARTITION_WRONG_NO_SUBPART_ERROR: Int = 1485

   /**
    * Constant, random or timezone-dependent expressions in (sub)partitioning function are not allowed
    */
   const val ER_WRONG_EXPR_IN_PARTITION_FUNC_ERROR: Int = 1486

   /**
    * Expression in RANGE/LIST VALUES must be constant
    */
   const val ER_NO_CONST_EXPR_IN_RANGE_OR_LIST_ERROR: Int = 1487

   /**
    * Field in list of fields for partition function not found in table
    */
   const val ER_FIELD_NOT_FOUND_PART_ERROR: Int = 1488

   /**
    * List of fields is only allowed in KEY partitions
    */
   const val ER_LIST_OF_FIELDS_ONLY_IN_HASH_ERROR: Int = 1489

   /**
    * The partition info in the frm file is not consistent with what can be written into the frm file
    */
   const val ER_INCONSISTENT_PARTITION_INFO_ERROR: Int = 1490

   /**
    * The %-.192s function returns the wrong type
    */
   const val ER_PARTITION_FUNC_NOT_ALLOWED_ERROR: Int = 1491

   /**
    * For %-.64s partitions each partition must be defined
    */
   const val ER_PARTITIONS_MUST_BE_DEFINED_ERROR: Int = 1492

   /**
    * VALUES LESS THAN value must be strictly increasing for each partition
    */
   const val ER_RANGE_NOT_INCREASING_ERROR: Int = 1493

   /**
    * VALUES value must be of same type as partition function
    */
   const val ER_INCONSISTENT_TYPE_OF_FUNCTIONS_ERROR: Int = 1494

   /**
    * Multiple definition of same constant in list partitioning
    */
   const val ER_MULTIPLE_DEF_CONST_IN_LIST_PART_ERROR: Int = 1495

   /**
    * Partitioning can not be used stand-alone in query
    */
   const val ER_PARTITION_ENTRY_ERROR: Int = 1496

   /**
    * The mix of handlers in the partitions is not allowed in this version of MySQL
    */
   const val ER_MIX_HANDLER_ERROR: Int = 1497

   /**
    * For the partitioned engine it is necessary to define all %-.64s
    */
   const val ER_PARTITION_NOT_DEFINED_ERROR: Int = 1498

   /**
    * Too many partitions (including subpartitions) were defined
    */
   const val ER_TOO_MANY_PARTITIONS_ERROR: Int = 1499

   /**
    * It is only possible to mix RANGE/LIST partitioning with HASH/KEY partitioning for subpartitioning
    */
   const val ER_SUBPARTITION_ERROR: Int = 1500

   /**
    * Failed to create specific handler file
    */
   const val ER_CANT_CREATE_HANDLER_FILE: Int = 1501

   /**
    * A BLOB field is not allowed in partition function
    */
   const val ER_BLOB_FIELD_IN_PART_FUNC_ERROR: Int = 1502

   /**
    * A %-.192s must include all columns in the table's partitioning function
    */
   const val ER_UNIQUE_KEY_NEED_ALL_FIELDS_IN_PF: Int = 1503

   /**
    * Number of %-.64s = 0 is not an allowed value
    */
   const val ER_NO_PARTS_ERROR: Int = 1504

   /**
    * Partition management on a not partitioned table is not possible
    */
   const val ER_PARTITION_MGMT_ON_NONPARTITIONED: Int = 1505

   /**
    * Foreign key clause is not yet supported in conjunction with partitioning
    */
   const val ER_FOREIGN_KEY_ON_PARTITIONED: Int = 1506

   /**
    * Error in list of partitions to %-.64s
    */
   const val ER_DROP_PARTITION_NON_EXISTENT: Int = 1507

   /**
    * Cannot remove all partitions, use DROP TABLE instead
    */
   const val ER_DROP_LAST_PARTITION: Int = 1508

   /**
    * COALESCE PARTITION can only be used on HASH/KEY partitions
    */
   const val ER_COALESCE_ONLY_ON_HASH_PARTITION: Int = 1509

   /**
    * REORGANIZE PARTITION can only be used to reorganize partitions not to change their numbers
    */
   const val ER_REORG_HASH_ONLY_ON_SAME_NO: Int = 1510

   /**
    * REORGANIZE PARTITION without parameters can only be used on auto-partitioned tables using HASH PARTITIONs
    */
   const val ER_REORG_NO_PARAM_ERROR: Int = 1511

   /**
    * %-.64s PARTITION can only be used on RANGE/LIST partitions
    */
   const val ER_ONLY_ON_RANGE_LIST_PARTITION: Int = 1512

   /**
    * Trying to Add partition(s) with wrong number of subpartitions
    */
   const val ER_ADD_PARTITION_SUBPART_ERROR: Int = 1513

   /**
    * At least one partition must be added
    */
   const val ER_ADD_PARTITION_NO_NEW_PARTITION: Int = 1514

   /**
    * At least one partition must be coalesced
    */
   const val ER_COALESCE_PARTITION_NO_PARTITION: Int = 1515

   /**
    * More partitions to reorganize than there are partitions
    */
   const val ER_REORG_PARTITION_NOT_EXIST: Int = 1516

   /**
    * Duplicate partition name %-.192s
    */
   const val ER_SAME_NAME_PARTITION: Int = 1517

   /**
    * It is not allowed to shut off binlog on this command
    */
   const val ER_NO_BINLOG_ERROR: Int = 1518

   /**
    * When reorganizing a set of partitions they must be in consecutive order
    */
   const val ER_CONSECUTIVE_REORG_PARTITIONS: Int = 1519

   /**
    * Reorganize of range partitions cannot change total ranges except for last partition where it can extend the
    * range
    */
   const val ER_REORG_OUTSIDE_RANGE: Int = 1520

   /**
    * Partition function not supported in this version for this handler
    */
   const val ER_PARTITION_FUNCTION_FAILURE: Int = 1521

   /**
    * Partition state cannot be defined from CREATE/ALTER TABLE
    */
   const val ER_PART_STATE_ERROR: Int = 1522

   /**
    * The %-.64s handler only supports 32 bit integers in VALUES
    */
   const val ER_LIMITED_PART_RANGE: Int = 1523

   /**
    * Plugin '%-.192s' is not loaded
    */
   const val ER_PLUGIN_IS_NOT_LOADED: Int = 1524

   /**
    * Incorrect %-.32s value: '%-.128s'
    */
   const val ER_WRONG_VALUE: Int = 1525

   /**
    * Table has no partition for value %-.64s
    */
   const val ER_NO_PARTITION_FOR_GIVEN_VALUE: Int = 1526

   /**
    * It is not allowed to specify %s more than once
    */
   const val ER_FILEGROUP_OPTION_ONLY_ONCE: Int = 1527

   /**
    * Failed to create %s
    */
   const val ER_CREATE_FILEGROUP_FAILED: Int = 1528

   /**
    * Failed to drop %s
    */
   const val ER_DROP_FILEGROUP_FAILED: Int = 1529

   /**
    * The handler doesn't support autoextend of tablespaces
    */
   const val ER_TABLESPACE_AUTO_EXTEND_ERROR: Int = 1530

   /**
    * A size parameter was incorrectly specified, either number or on the form 10M
    */
   const val ER_WRONG_SIZE_NUMBER: Int = 1531

   /**
    * The size number was correct but we don't allow the digit part to be more than 2 billion
    */
   const val ER_SIZE_OVERFLOW_ERROR: Int = 1532

   /**
    * Failed to alter: %s
    */
   const val ER_ALTER_FILEGROUP_FAILED: Int = 1533

   /**
    * Writing one row to the row-based binary log failed
    */
   const val ER_BINLOG_ROW_LOGGING_FAILED: Int = 1534

   /**
    * Table definition on master and slave does not match: %s
    */
   const val ER_BINLOG_ROW_WRONG_TABLE_DEF: Int = 1535

   /**
    * Slave running with --log-slave-updates must use row-based binary logging to be able to replicate row-based
    * binary log events
    */
   const val ER_BINLOG_ROW_RBR_TO_SBR: Int = 1536

   /**
    * Event '%-.192s' already exists
    */
   const val ER_EVENT_ALREADY_EXISTS: Int = 1537

   /**
    * Failed to store event %s. Error code %d from storage engine.
    */
   const val ER_EVENT_STORE_FAILED: Int = 1538

   /**
    * Unknown event '%-.192s'
    */
   const val ER_EVENT_DOES_NOT_EXIST: Int = 1539

   /**
    * Failed to alter event '%-.192s'
    */
   const val ER_EVENT_CANT_ALTER: Int = 1540

   /**
    * Failed to drop %s
    */
   const val ER_EVENT_DROP_FAILED: Int = 1541

   /**
    * INTERVAL is either not positive or too big
    */
   const val ER_EVENT_INTERVAL_NOT_POSITIVE_OR_TOO_BIG: Int = 1542

   /**
    * ENDS is either invalid or before STARTS
    */
   const val ER_EVENT_ENDS_BEFORE_STARTS: Int = 1543

   /**
    * Event execution time is in the past. Event has been disabled
    */
   const val ER_EVENT_EXEC_TIME_IN_THE_PAST: Int = 1544

   /**
    * Failed to open mysql.event
    */
   const val ER_EVENT_OPEN_TABLE_FAILED: Int = 1545

   /**
    * No datetime expression provided
    */
   const val ER_EVENT_NEITHER_M_EXPR_NOR_M_AT: Int = 1546

   /**
    * Column count of mysql.%s is wrong. Expected %d, found %d. The table is probably corrupted
    */
   const val ER_OBSOLETE_COL_COUNT_DOESNT_MATCH_CORRUPTED: Int = 1547

   /**
    * Cannot load from mysql.%s. The table is probably corrupted
    */
   const val ER_OBSOLETE_CANNOT_LOAD_FROM_TABLE: Int = 1548

   /**
    * Failed to delete the event from mysql.event
    */
   const val ER_EVENT_CANNOT_DELETE: Int = 1549

   /**
    * Error during compilation of event's body
    */
   const val ER_EVENT_COMPILE_ERROR: Int = 1550

   /**
    * Same old and new event name
    */
   const val ER_EVENT_SAME_NAME: Int = 1551

   /**
    * Data for column '%s' too long
    */
   const val ER_EVENT_DATA_TOO_LONG: Int = 1552

   /**
    * Cannot drop index '%-.192s': needed in a foreign key constraint
    */
   const val ER_DROP_INDEX_FK: Int = 1553

   /**
    * The syntax '%s' is deprecated and will be removed in MySQL %s. Please use %s instead
    */
   const val ER_WARN_DEPRECATED_SYNTAX_WITH_VER: Int = 1554

   /**
    * You can't write-lock a log table. Only read access is possible
    */
   const val ER_CANT_WRITE_LOCK_LOG_TABLE: Int = 1555

   /**
    * You can't use locks with log tables.
    */
   const val ER_CANT_LOCK_LOG_TABLE: Int = 1556

   /**
    * Upholding foreign key constraints for table '%.192s', entry '%-.192s', key %d would lead to a duplicate entry
    */
   const val ER_FOREIGN_DUPLICATE_KEY_OLD_UNUSED: Int = 1557

   /**
    * Column count of mysql.%s is wrong. Expected %d, found %d. Created with MySQL %d, now running %d. Please use
    * mysql_upgrade to fix this error.
    */
   const val ER_COL_COUNT_DOESNT_MATCH_PLEASE_UPDATE: Int = 1558

   /**
    * Cannot switch out of the row-based binary log format when the session has open temporary tables
    */
   const val ER_TEMP_TABLE_PREVENTS_SWITCH_OUT_OF_RBR: Int = 1559

   /**
    * Cannot change the binary logging format inside a stored function or trigger
    */
   const val ER_STORED_FUNCTION_PREVENTS_SWITCH_BINLOG_FORMAT: Int = 1560

   /**
    * The NDB cluster engine does not support changing the binlog format on the fly yet
    */
   const val ER_NDB_CANT_SWITCH_BINLOG_FORMAT: Int = 1561

   /**
    * Cannot create temporary table with partitions
    */
   const val ER_PARTITION_NO_TEMPORARY: Int = 1562

   /**
    * Partition constant is out of partition function domain
    */
   const val ER_PARTITION_CONST_DOMAIN_ERROR: Int = 1563

   /**
    * This partition function is not allowed
    */
   const val ER_PARTITION_FUNCTION_IS_NOT_ALLOWED: Int = 1564

   /**
    * Error in DDL log
    */
   const val ER_DDL_LOG_ERROR: Int = 1565

   /**
    * Not allowed to use NULL value in VALUES LESS THAN
    */
   const val ER_NULL_IN_VALUES_LESS_THAN: Int = 1566

   /**
    * Incorrect partition name
    */
   const val ER_WRONG_PARTITION_NAME: Int = 1567

   /**
    * Transaction characteristics can't be changed while a transaction is in progress
    */
   const val ER_CANT_CHANGE_TX_CHARACTERISTICS: Int = 1568

   /**
    * ALTER TABLE causes auto_increment resequencing, resulting in duplicate entry '%-.192s' for key '%-.192s'
    */
   const val ER_DUP_ENTRY_AUTOINCREMENT_CASE: Int = 1569

   /**
    * Internal scheduler error %d
    */
   const val ER_EVENT_MODIFY_QUEUE_ERROR: Int = 1570

   /**
    * Error during starting/stopping of the scheduler. Error code %u
    */
   const val ER_EVENT_SET_VAR_ERROR: Int = 1571

   /**
    * Engine cannot be used in partitioned tables
    */
   const val ER_PARTITION_MERGE_ERROR: Int = 1572

   /**
    * Cannot activate '%-.64s' log
    */
   const val ER_CANT_ACTIVATE_LOG: Int = 1573

   /**
    * The server was not built with row-based replication
    */
   const val ER_RBR_NOT_AVAILABLE: Int = 1574

   /**
    * Decoding of base64 string failed
    */
   const val ER_BASE64_DECODE_ERROR: Int = 1575

   /**
    * Recursion of EVENT DDL statements is forbidden when body is present
    */
   const val ER_EVENT_RECURSION_FORBIDDEN: Int = 1576

   /**
    * Cannot proceed because system tables used by Event Scheduler were found damaged at server start
    */
   const val ER_EVENTS_DB_ERROR: Int = 1577

   /**
    * Only integers allowed as number here
    */
   const val ER_ONLY_INTEGERS_ALLOWED: Int = 1578

   /**
    * This storage engine cannot be used for log tables"
    */
   const val ER_UNSUPORTED_LOG_ENGINE: Int = 1579

   /**
    * You cannot '%s' a log table if logging is enabled
    */
   const val ER_BAD_LOG_STATEMENT: Int = 1580

   /**
    * Cannot rename '%s'. When logging enabled, rename to/from log table must rename two tables: the log table to an
    * archive table and another table back to '%s'
    */
   const val ER_CANT_RENAME_LOG_TABLE: Int = 1581

   /**
    * Incorrect parameter count in the call to native function '%-.192s'
    */
   const val ER_WRONG_PARAMCOUNT_TO_NATIVE_FCT: Int = 1582

   /**
    * Incorrect parameters in the call to native function '%-.192s'
    */
   const val ER_WRONG_PARAMETERS_TO_NATIVE_FCT: Int = 1583

   /**
    * Incorrect parameters in the call to stored function '%-.192s'
    */
   const val ER_WRONG_PARAMETERS_TO_STORED_FCT: Int = 1584

   /**
    * This function '%-.192s' has the same name as a native function
    */
   const val ER_NATIVE_FCT_NAME_COLLISION: Int = 1585

   /**
    * Duplicate entry '%-.64s' for key '%-.192s'
    */
   const val ER_DUP_ENTRY_WITH_KEY_NAME: Int = 1586

   /**
    * Too many files opened, please execute the command again
    */
   const val ER_BINLOG_PURGE_EMFILE: Int = 1587

   /**
    * Event execution time is in the past and ON COMPLETION NOT PRESERVE is set. The event was dropped immediately
    * after creation.
    */
   const val ER_EVENT_CANNOT_CREATE_IN_THE_PAST: Int = 1588

   /**
    * Event execution time is in the past and ON COMPLETION NOT PRESERVE is set. The event was not changed. Specify
    * a time in the future.
    */
   const val ER_EVENT_CANNOT_ALTER_IN_THE_PAST: Int = 1589

   /**
    * The incident %s occured on the master. Message: %-.64s
    */
   const val ER_SLAVE_INCIDENT: Int = 1590

   /**
    * Table has no partition for some existing values
    */
   const val ER_NO_PARTITION_FOR_GIVEN_VALUE_SILENT: Int = 1591

   /**
    * Unsafe statement written to the binary log using statement format since BINLOG_FORMAT = STATEMENT. %s
    */
   const val ER_BINLOG_UNSAFE_STATEMENT: Int = 1592

   /**
    * Fatal error: %s
    */
   const val ER_SLAVE_FATAL_ERROR: Int = 1593

   /**
    * Relay log read failure: %s
    */
   const val ER_SLAVE_RELAY_LOG_READ_FAILURE: Int = 1594

   /**
    * Relay log write failure: %s
    */
   const val ER_SLAVE_RELAY_LOG_WRITE_FAILURE: Int = 1595

   /**
    * Failed to create %s
    */
   const val ER_SLAVE_CREATE_EVENT_FAILURE: Int = 1596

   /**
    * Master command %s failed: %s
    */
   const val ER_SLAVE_MASTER_COM_FAILURE: Int = 1597

   /**
    * Binary logging not possible. Message: %s
    */
   const val ER_BINLOG_LOGGING_IMPOSSIBLE: Int = 1598

   /**
    * View `%-.64s`.`%-.64s` has no creation context
    */
   const val ER_VIEW_NO_CREATION_CTX: Int = 1599

   /**
    * Creation context of view `%-.64s`.`%-.64s' is invalid
    */
   const val ER_VIEW_INVALID_CREATION_CTX: Int = 1600

   /**
    * Creation context of stored routine `%-.64s`.`%-.64s` is invalid
    */
   const val ER_SR_INVALID_CREATION_CTX: Int = 1601

   /**
    * Corrupted TRG file for table `%-.64s`.`%-.64s`
    */
   const val ER_TRG_CORRUPTED_FILE: Int = 1602

   /**
    * Triggers for table `%-.64s`.`%-.64s` have no creation context
    */
   const val ER_TRG_NO_CREATION_CTX: Int = 1603

   /**
    * Trigger creation context of table `%-.64s`.`%-.64s` is invalid
    */
   const val ER_TRG_INVALID_CREATION_CTX: Int = 1604

   /**
    * Creation context of event `%-.64s`.`%-.64s` is invalid
    */
   const val ER_EVENT_INVALID_CREATION_CTX: Int = 1605

   /**
    * Cannot open table for trigger `%-.64s`.`%-.64s`
    */
   const val ER_TRG_CANT_OPEN_TABLE: Int = 1606

   /**
    * Cannot create stored routine `%-.64s`. Check warnings
    */
   const val ER_CANT_CREATE_SROUTINE: Int = 1607

   /**
    * Ambiguous slave modes combination. %s
    */
   const val ER_NEVER_USED: Int = 1608

   /**
    * The BINLOG statement of type `%s` was not preceded by a format description BINLOG statement.
    */
   const val ER_NO_FORMAT_DESCRIPTION_EVENT_BEFORE_BINLOG_STATEMENT: Int = 1609

   /**
    * Corrupted replication event was detected
    */
   const val ER_SLAVE_CORRUPT_EVENT: Int = 1610

   /**
    * Invalid column reference (%-.64s) in LOAD DATA
    */
   const val ER_LOAD_DATA_INVALID_COLUMN: Int = 1611

   /**
    * Being purged log %s was not found
    */
   const val ER_LOG_PURGE_NO_FILE: Int = 1612

   /**
    * XA_RBTIMEOUT: Transaction branch was rolled back: took too long
    */
   const val ER_XA_RBTIMEOUT: Int = 1613

   /**
    * XA_RBDEADLOCK: Transaction branch was rolled back: deadlock was detected
    */
   const val ER_XA_RBDEADLOCK: Int = 1614

   /**
    * Prepared statement needs to be re-prepared
    */
   const val ER_NEED_REPREPARE: Int = 1615

   /**
    * DELAYED option not supported for table '%-.192s'
    */
   const val ER_DELAYED_NOT_SUPPORTED: Int = 1616

   /**
    * The master info structure does not exist
    */
   const val WARN_NO_MASTER_INFO: Int = 1617

   /**
    * %-.64s option ignored
    */
   const val WARN_OPTION_IGNORED: Int = 1618

   /**
    * Built-in plugins cannot be deleted
    */
   const val WARN_PLUGIN_DELETE_BUILTIN: Int = 1619

   /**
    * Plugin is busy and will be uninstalled on shutdown
    */
   const val WARN_PLUGIN_BUSY: Int = 1620

   /**
    * %s variable '%s' is read-only. Use SET %s to assign the value
    */
   const val ER_VARIABLE_IS_READONLY: Int = 1621

   /**
    * Storage engine %s does not support rollback for this statement. Transaction rolled back and must be restarted
    */
   const val ER_WARN_ENGINE_TRANSACTION_ROLLBACK: Int = 1622

   /**
    * Unexpected master's heartbeat data: %s
    */
   const val ER_SLAVE_HEARTBEAT_FAILURE: Int = 1623

   /**
    * The requested value for the heartbeat period is either negative or exceeds the maximum allowed (%s seconds).
    */
   const val ER_SLAVE_HEARTBEAT_VALUE_OUT_OF_RANGE: Int = 1624

   /**
    * Bad schema for mysql.ndb_replication table. Message: %-.64s
    */
   const val ER_NDB_REPLICATION_SCHEMA_ERROR: Int = 1625

   /**
    * Error in parsing conflict function. Message: %-.64s
    */
   const val ER_CONFLICT_FN_PARSE_ERROR: Int = 1626

   /**
    * Write to exceptions table failed. Message: %-.128s"
    */
   const val ER_EXCEPTIONS_WRITE_ERROR: Int = 1627

   /**
    * Comment for table '%-.64s' is too long (max = %lu)
    */
   const val ER_TOO_LONG_TABLE_COMMENT: Int = 1628

   /**
    * Comment for field '%-.64s' is too long (max = %lu)
    */
   const val ER_TOO_LONG_FIELD_COMMENT: Int = 1629

   /**
    * FUNCTION %s does not exist. Check the 'Function Name Parsing and Resolution' section in the Reference Manual
    */
   const val ER_FUNC_INEXISTENT_NAME_COLLISION: Int = 1630

   /**
    * Database
    */
   const val ER_DATABASE_NAME: Int = 1631

   /**
    * Table
    */
   const val ER_TABLE_NAME: Int = 1632

   /**
    * Partition
    */
   const val ER_PARTITION_NAME: Int = 1633

   /**
    * Subpartition
    */
   const val ER_SUBPARTITION_NAME: Int = 1634

   /**
    * Temporary
    */
   const val ER_TEMPORARY_NAME: Int = 1635

   /**
    * Renamed
    */
   const val ER_RENAMED_NAME: Int = 1636

   /**
    * Too many active concurrent transactions
    */
   const val ER_TOO_MANY_CONCURRENT_TRXS: Int = 1637

   /**
    * Non-ASCII separator arguments are not fully supported
    */
   const val WARN_NON_ASCII_SEPARATOR_NOT_IMPLEMENTED: Int = 1638

   /**
    * Debug sync point wait timed out
    */
   const val ER_DEBUG_SYNC_TIMEOUT: Int = 1639

   /**
    * Debug sync point hit limit reached
    */
   const val ER_DEBUG_SYNC_HIT_LIMIT: Int = 1640

   /**
    * Duplicate condition information item '%s'
    */
   const val ER_DUP_SIGNAL_SET: Int = 1641

   /**
    * Unhandled user-defined warning condition
    */
   const val ER_SIGNAL_WARN: Int = 1642

   /**
    * Unhandled user-defined not found condition
    */
   const val ER_SIGNAL_NOT_FOUND: Int = 1643

   /**
    * Unhandled user-defined exception condition
    */
   const val ER_SIGNAL_EXCEPTION: Int = 1644

   /**
    * RESIGNAL when handler not active
    */
   const val ER_RESIGNAL_WITHOUT_ACTIVE_HANDLER: Int = 1645

   /**
    * SIGNAL/RESIGNAL can only use a CONDITION defined with SQLSTATE
    */
   const val ER_SIGNAL_BAD_CONDITION_TYPE: Int = 1646

   /**
    * Data truncated for condition item '%s'
    */
   const val WARN_COND_ITEM_TRUNCATED: Int = 1647

   /**
    * Data too long for condition item '%s'
    */
   const val ER_COND_ITEM_TOO_LONG: Int = 1648

   /**
    * Unknown locale: '%-.64s'
    */
   const val ER_UNKNOWN_LOCALE: Int = 1649

   /**
    * The requested server id %d clashes with the slave startup option --replicate-same-server-id
    */
   const val ER_SLAVE_IGNORE_SERVER_IDS: Int = 1650

   /**
    * Query cache is disabled; restart the server with query_cache_type=1 to enable it
    */
   const val ER_QUERY_CACHE_DISABLED: Int = 1651

   /**
    * Duplicate partition field name '%-.192s'
    */
   const val ER_SAME_NAME_PARTITION_FIELD: Int = 1652

   /**
    * Inconsistency in usage of column lists for partitioning
    */
   const val ER_PARTITION_COLUMN_LIST_ERROR: Int = 1653

   /**
    * Partition column values of incorrect type
    */
   const val ER_WRONG_TYPE_COLUMN_VALUE_ERROR: Int = 1654

   /**
    * Too many fields in '%-.192s'
    */
   const val ER_TOO_MANY_PARTITION_FUNC_FIELDS_ERROR: Int = 1655

   /**
    * Cannot use MAXVALUE as value in VALUES IN
    */
   const val ER_MAXVALUE_IN_VALUES_IN: Int = 1656

   /**
    * Cannot have more than one value for this type of %-.64s partitioning
    */
   const val ER_TOO_MANY_VALUES_ERROR: Int = 1657

   /**
    * Row expressions in VALUES IN only allowed for multi-field column partitioning
    */
   const val ER_ROW_SINGLE_PARTITION_FIELD_ERROR: Int = 1658

   /**
    * Field '%-.192s' is of a not allowed type for this type of partitioning
    */
   const val ER_FIELD_TYPE_NOT_ALLOWED_AS_PARTITION_FIELD: Int = 1659

   /**
    * The total length of the partitioning fields is too large
    */
   const val ER_PARTITION_FIELDS_TOO_LONG: Int = 1660

   /**
    * Cannot execute statement: impossible to write to binary log since both row-incapable engines and
    * statement-incapable engines are involved.
    */
   const val ER_BINLOG_ROW_ENGINE_AND_STMT_ENGINE: Int = 1661

   /**
    * Cannot execute statement: impossible to write to binary log since BINLOG_FORMAT = ROW and at least one table
    * uses a storage engine limited to statement-based logging.
    */
   const val ER_BINLOG_ROW_MODE_AND_STMT_ENGINE: Int = 1662

   /**
    * Cannot execute statement: impossible to write to binary log since statement is unsafe, storage engine is
    * limited to statement-based logging, and BINLOG_FORMAT = MIXED. %s
    */
   const val ER_BINLOG_UNSAFE_AND_STMT_ENGINE: Int = 1663

   /**
    * Cannot execute statement: impossible to write to binary log since statement is in row format and at least one
    * table uses a storage engine limited to statement-based logging.
    */
   const val ER_BINLOG_ROW_INJECTION_AND_STMT_ENGINE: Int = 1664

   /**
    * Cannot execute statement: impossible to write to binary log since BINLOG_FORMAT = STATEMENT and at least one
    * table uses a storage engine limited to row-based logging.%s
    */
   const val ER_BINLOG_STMT_MODE_AND_ROW_ENGINE: Int = 1665

   /**
    * Cannot execute statement: impossible to write to binary log since statement is in row format and BINLOG_FORMAT
    * = STATEMENT.
    */
   const val ER_BINLOG_ROW_INJECTION_AND_STMT_MODE: Int = 1666

   /**
    * Cannot execute statement: impossible to write to binary log since more than one engine is involved and at
    * least one engine is self-logging.
    */
   const val ER_BINLOG_MULTIPLE_ENGINES_AND_SELF_LOGGING_ENGINE: Int = 1667

   /**
    * The statement is unsafe because it uses a LIMIT clause. This is unsafe because the set of rows included cannot
    * be predicted.
    */
   const val ER_BINLOG_UNSAFE_LIMIT: Int = 1668

   /**
    * The statement is unsafe because it uses INSERT DELAYED. This is unsafe because the times when rows are
    * inserted cannot be predicted.
    */
   const val ER_BINLOG_UNSAFE_INSERT_DELAYED: Int = 1669

   /**
    * The statement is unsafe because it uses the general log, slow query log, or performance_schema table(s). This
    * is unsafe because system tables may differ on slaves.
    */
   const val ER_BINLOG_UNSAFE_SYSTEM_TABLE: Int = 1670

   /**
    * Statement is unsafe because it invokes a trigger or a stored function that inserts into an AUTO_INCREMENT
    * column. Inserted values cannot be logged correctly.
    */
   const val ER_BINLOG_UNSAFE_AUTOINC_COLUMNS: Int = 1671

   /**
    * Statement is unsafe because it uses a UDF which may not return the same value on the slave.
    */
   const val ER_BINLOG_UNSAFE_UDF: Int = 1672

   /**
    * Statement is unsafe because it uses a system variable that may have a different value on the slave.
    */
   const val ER_BINLOG_UNSAFE_SYSTEM_VARIABLE: Int = 1673

   /**
    * Statement is unsafe because it uses a system function that may return a different value on the slave.
    */
   const val ER_BINLOG_UNSAFE_SYSTEM_FUNCTION: Int = 1674

   /**
    * Statement is unsafe because it accesses a non-transactional table after accessing a transactional table within
    * the same transaction.
    */
   const val ER_BINLOG_UNSAFE_NONTRANS_AFTER_TRANS: Int = 1675

   /**
    * %s Statement: %s
    */
   const val ER_MESSAGE_AND_STATEMENT: Int = 1676

   /**
    * Column %d of table '%-.192s.%-.192s' cannot be converted from type '%-.32s' to type '%-.32s'
    */
   const val ER_SLAVE_CONVERSION_FAILED: Int = 1677

   /**
    * Can't create conversion table for table '%-.192s.%-.192s'
    */
   const val ER_SLAVE_CANT_CREATE_CONVERSION: Int = 1678

   /**
    * Cannot modify @@session.binlog_format inside a transaction
    */
   const val ER_INSIDE_TRANSACTION_PREVENTS_SWITCH_BINLOG_FORMAT: Int = 1679

   /**
    * The path specified for %.64s is too long.
    */
   const val ER_PATH_LENGTH: Int = 1680

   /**
    * '%s' is deprecated and will be removed in a future release.
    */
   const val ER_WARN_DEPRECATED_SYNTAX_NO_REPLACEMENT: Int = 1681

   /**
    * Native table '%-.64s'.'%-.64s' has the wrong structure
    */
   const val ER_WRONG_NATIVE_TABLE_STRUCTURE: Int = 1682

   /**
    * Invalid performance_schema usage.
    */
   const val ER_WRONG_PERFSCHEMA_USAGE: Int = 1683

   /**
    * Table '%s'.'%s' was skipped since its definition is being modified by concurrent DDL statement
    */
   const val ER_WARN_I_S_SKIPPED_TABLE: Int = 1684

   /**
    * Cannot modify @@session.binlog_direct_non_transactional_updates inside a transaction
    */
   const val ER_INSIDE_TRANSACTION_PREVENTS_SWITCH_BINLOG_DIRECT: Int = 1685

   /**
    * Cannot change the binlog direct flag inside a stored function or trigger
    */
   const val ER_STORED_FUNCTION_PREVENTS_SWITCH_BINLOG_DIRECT: Int = 1686

   /**
    * A SPATIAL index may only contain a geometrical type column
    */
   const val ER_SPATIAL_MUST_HAVE_GEOM_COL: Int = 1687

   /**
    * Comment for index '%-.64s' is too long (max = %lu)
    */
   const val ER_TOO_LONG_INDEX_COMMENT: Int = 1688

   /**
    * Wait on a lock was aborted due to a pending exclusive lock
    */
   const val ER_LOCK_ABORTED: Int = 1689

   /**
    * %s value is out of range in '%s'
    */
   const val ER_DATA_OUT_OF_RANGE: Int = 1690

   /**
    * A variable of a non-integer based type in LIMIT clause
    */
   const val ER_WRONG_SPVAR_TYPE_IN_LIMIT: Int = 1691

   /**
    * Mixing self-logging and non-self-logging engines in a statement is unsafe.
    */
   const val ER_BINLOG_UNSAFE_MULTIPLE_ENGINES_AND_SELF_LOGGING_ENGINE: Int = 1692

   /**
    * Statement accesses nontransactional table as well as transactional or temporary table, and writes to any of
    * them.
    */
   const val ER_BINLOG_UNSAFE_MIXED_STATEMENT: Int = 1693

   /**
    * Cannot modify @@session.sql_log_bin inside a transaction
    */
   const val ER_INSIDE_TRANSACTION_PREVENTS_SWITCH_SQL_LOG_BIN: Int = 1694

   /**
    * Cannot change the sql_log_bin inside a stored function or trigger
    */
   const val ER_STORED_FUNCTION_PREVENTS_SWITCH_SQL_LOG_BIN: Int = 1695

   /**
    * Failed to read from the .par file
    */
   const val ER_FAILED_READ_FROM_PAR_FILE: Int = 1696

   /**
    * VALUES value for partition '%-.64s' must have type INT
    */
   const val ER_VALUES_IS_NOT_INT_TYPE_ERROR: Int = 1697

   /**
    * Access denied for user '%-.48s'@'%-.64s'
    */
   const val ER_ACCESS_DENIED_NO_PASSWORD_ERROR: Int = 1698

   /**
    * SET PASSWORD has no significance for users authenticating via plugins
    */
   const val ER_SET_PASSWORD_AUTH_PLUGIN: Int = 1699

   /**
    * GRANT with IDENTIFIED WITH is illegal because the user %-.*s already exists
    */
   const val ER_GRANT_PLUGIN_USER_EXISTS: Int = 1700

   /**
    * Cannot truncate a table referenced in a foreign key constraint (%.192s)
    */
   const val ER_TRUNCATE_ILLEGAL_FK: Int = 1701

   /**
    * Plugin '%s' is force_plus_permanent and can not be unloaded
    */
   const val ER_PLUGIN_IS_PERMANENT: Int = 1702

   /**
    * The requested value for the heartbeat period is less than 1 millisecond. The value is reset to 0, meaning that
    * heartbeating will effectively be disabled.
    */
   const val ER_SLAVE_HEARTBEAT_VALUE_OUT_OF_RANGE_MIN: Int = 1703

   /**
    * The requested value for the heartbeat period exceeds the value of `slave_net_timeout' seconds. A sensible
    * value for the period should be less than the timeout.
    */
   const val ER_SLAVE_HEARTBEAT_VALUE_OUT_OF_RANGE_MAX: Int = 1704

   /**
    * Multi-row statements required more than 'max_binlog_stmt_cache_size' bytes of storage; increase this mysqld
    * variable and try again
    */
   const val ER_STMT_CACHE_FULL: Int = 1705

   /**
    * Primary key/partition key update is not allowed since the table is updated both as '%-.192s' and '%-.192s'.
    */
   const val ER_MULTI_UPDATE_KEY_CONFLICT: Int = 1706

   /**
    * Table rebuild required. Please do \"ALTER TABLE `%-.32s` FORCE\" or dump/reload to fix it!
    */
   const val ER_TABLE_NEEDS_REBUILD: Int = 1707

   /**
    * The value of '%s' should be no less than the value of '%s'
    */
   const val WARN_OPTION_BELOW_LIMIT: Int = 1708

   /**
    * Index column size too large. The maximum column size is %lu bytes.
    */
   const val ER_INDEX_COLUMN_TOO_LONG: Int = 1709

   /**
    * Trigger '%-.64s' has an error in its body: '%-.256s'
    */
   const val ER_ERROR_IN_TRIGGER_BODY: Int = 1710

   /**
    * Unknown trigger has an error in its body: '%-.256s'
    */
   const val ER_ERROR_IN_UNKNOWN_TRIGGER_BODY: Int = 1711

   /**
    * Index %s is corrupted
    */
   const val ER_INDEX_CORRUPT: Int = 1712

   /**
    * Undo log record is too big.
    */
   const val ER_UNDO_RECORD_TOO_BIG: Int = 1713

   /**
    * INSERT IGNORE... SELECT is unsafe because the order in which rows are retrieved by the SELECT determines which
    * (if any) rows are ignored. This order cannot be predicted and may differ on master and the slave.
    */
   const val ER_BINLOG_UNSAFE_INSERT_IGNORE_SELECT: Int = 1714

   /**
    * INSERT... SELECT... ON DUPLICATE KEY UPDATE is unsafe because the order in which rows are retrieved by the
    * SELECT determines which (if any) rows are updated. This order cannot be predicted and may differ on master and
    * the slave.
    */
   const val ER_BINLOG_UNSAFE_INSERT_SELECT_UPDATE: Int = 1715

   /**
    * REPLACE... SELECT is unsafe because the order in which rows are retrieved by the SELECT determines which (if
    * any) rows are replaced. This order cannot be predicted and may differ on master and the slave.
    */
   const val ER_BINLOG_UNSAFE_REPLACE_SELECT: Int = 1716

   /**
    * CREATE... IGNORE SELECT is unsafe because the order in which rows are retrieved by the SELECT determines which
    * (if any) rows are ignored. This order cannot be predicted and may differ on master and the slave.
    */
   const val ER_BINLOG_UNSAFE_CREATE_IGNORE_SELECT: Int = 1717

   /**
    * CREATE... REPLACE SELECT is unsafe because the order in which rows are retrieved by the SELECT determines
    * which (if any) rows are replaced. This order cannot be predicted and may differ on master and the slave.
    */
   const val ER_BINLOG_UNSAFE_CREATE_REPLACE_SELECT: Int = 1718

   /**
    * UPDATE IGNORE is unsafe because the order in which rows are updated determines which (if any) rows are
    * ignored. This order cannot be predicted and may differ on master and the slave.
    */
   const val ER_BINLOG_UNSAFE_UPDATE_IGNORE: Int = 1719

   /**
    * Plugin '%s' is marked as not dynamically uninstallable. You have to stop the server to uninstall it.
    */
   const val ER_PLUGIN_NO_UNINSTALL: Int = 1720

   /**
    * Plugin '%s' is marked as not dynamically installable. You have to stop the server to install it.
    */
   const val ER_PLUGIN_NO_INSTALL: Int = 1721

   /**
    * Statements writing to a table with an auto-increment column after selecting from another table are unsafe
    * because the order in which rows are retrieved determines what (if any) rows will be written. This order cannot
    * be predicted and may differ on master and the slave.
    */
   const val ER_BINLOG_UNSAFE_WRITE_AUTOINC_SELECT: Int = 1722

   /**
    * CREATE TABLE... SELECT...  on a table with an auto-increment column is unsafe because the order in which rows
    * are retrieved by the SELECT determines which (if any) rows are inserted. This order cannot be predicted and
    * may differ on master and the slave.
    */
   const val ER_BINLOG_UNSAFE_CREATE_SELECT_AUTOINC: Int = 1723

   /**
    * INSERT... ON DUPLICATE KEY UPDATE  on a table with more than one UNIQUE KEY is unsafe
    */
   const val ER_BINLOG_UNSAFE_INSERT_TWO_KEYS: Int = 1724

   /**
    * Table is being used in foreign key check.
    */
   const val ER_TABLE_IN_FK_CHECK: Int = 1725

   /**
    * Storage engine '%s' does not support system tables. [%s.%s]
    */
   const val ER_UNSUPPORTED_ENGINE: Int = 1726

   /**
    * INSERT into autoincrement field which is not the first part in the composed primary key is unsafe.
    */
   const val ER_BINLOG_UNSAFE_AUTOINC_NOT_FIRST: Int = 1727

   /**
    * Cannot load from %s.%s. The table is probably corrupted
    */
   const val ER_CANNOT_LOAD_FROM_TABLE_V2: Int = 1728

   /**
    * The requested value %s for the master delay exceeds the maximum %u
    */
   const val ER_MASTER_DELAY_VALUE_OUT_OF_RANGE: Int = 1729

   /**
    * Only Format_description_log_event and row events are allowed in BINLOG statements (but %s was provided)
    */
   const val ER_ONLY_FD_AND_RBR_EVENTS_ALLOWED_IN_BINLOG_STATEMENT: Int = 1730

   /**
    * Non matching attribute '%-.64s' between partition and table
    */
   const val ER_PARTITION_EXCHANGE_DIFFERENT_OPTION: Int = 1731

   /**
    * Table to exchange with partition is partitioned: '%-.64s'
    */
   const val ER_PARTITION_EXCHANGE_PART_TABLE: Int = 1732

   /**
    * Table to exchange with partition is temporary: '%-.64s'
    */
   const val ER_PARTITION_EXCHANGE_TEMP_TABLE: Int = 1733

   /**
    * Subpartitioned table, use subpartition instead of partition
    */
   const val ER_PARTITION_INSTEAD_OF_SUBPARTITION: Int = 1734

   /**
    * Unknown partition '%-.64s' in table '%-.64s'
    */
   const val ER_UNKNOWN_PARTITION: Int = 1735

   /**
    * Tables have different definitions
    */
   const val ER_TABLES_DIFFERENT_METADATA: Int = 1736

   /**
    * Found a row that does not match the partition
    */
   const val ER_ROW_DOES_NOT_MATCH_PARTITION: Int = 1737

   /**
    * Option binlog_cache_size (%lu) is greater than max_binlog_cache_size (%lu); setting binlog_cache_size equal to
    * max_binlog_cache_size.
    */
   const val ER_BINLOG_CACHE_SIZE_GREATER_THAN_MAX: Int = 1738

   /**
    * Cannot use %-.64s access on index '%-.64s' due to type or collation conversion on field '%-.64s'
    */
   const val ER_WARN_INDEX_NOT_APPLICABLE: Int = 1739

   /**
    * Table to exchange with partition has foreign key references: '%-.64s'
    */
   const val ER_PARTITION_EXCHANGE_FOREIGN_KEY: Int = 1740

   /**
    * Key value '%-.192s' was not found in table '%-.192s.%-.192s'
    */
   const val ER_NO_SUCH_KEY_VALUE: Int = 1741

   /**
    * Data for column '%s' too long
    */
   const val ER_RPL_INFO_DATA_TOO_LONG: Int = 1742

   /**
    * Replication event checksum verification failed while reading from network.
    */
   const val ER_NETWORK_READ_EVENT_CHECKSUM_FAILURE: Int = 1743

   /**
    * Replication event checksum verification failed while reading from a log file.
    */
   const val ER_BINLOG_READ_EVENT_CHECKSUM_FAILURE: Int = 1744

   /**
    * Option binlog_stmt_cache_size (%lu) is greater than max_binlog_stmt_cache_size (%lu); setting
    * binlog_stmt_cache_size equal to max_binlog_stmt_cache_size.
    */
   const val ER_BINLOG_STMT_CACHE_SIZE_GREATER_THAN_MAX: Int = 1745

   /**
    * Can't update table '%-.192s' while '%-.192s' is being created.
    */
   const val ER_CANT_UPDATE_TABLE_IN_CREATE_TABLE_SELECT: Int = 1746

   /**
    * PARTITION () clause on non partitioned table
    */
   const val ER_PARTITION_CLAUSE_ON_NONPARTITIONED: Int = 1747

   /**
    * Found a row not matching the given partition set
    */
   const val ER_ROW_DOES_NOT_MATCH_GIVEN_PARTITION_SET: Int = 1748

   /**
    * Partition '%-.64s' doesn't exist
    */
   // checkstyle, please ignore ConstantName for the next line
   const val ER_NO_SUCH_PARTITION__UNUSED: Int = 1749

   /**
    * Failure while changing the type of replication repository: %s.
    */
   const val ER_CHANGE_RPL_INFO_REPOSITORY_FAILURE: Int = 1750

   /**
    * The creation of some temporary tables could not be rolled back.
    */
   const val ER_WARNING_NOT_COMPLETE_ROLLBACK_WITH_CREATED_TEMP_TABLE: Int = 1751

   /**
    * Some temporary tables were dropped, but these operations could not be rolled back.
    */
   const val ER_WARNING_NOT_COMPLETE_ROLLBACK_WITH_DROPPED_TEMP_TABLE: Int = 1752

   /**
    * %s is not supported in multi-threaded slave mode. %s
    */
   const val ER_MTS_FEATURE_IS_NOT_SUPPORTED: Int = 1753

   /**
    * The number of modified databases exceeds the maximum %d; the database names will not be included in the
    * replication event metadata.
    */
   const val ER_MTS_UPDATED_DBS_GREATER_MAX: Int = 1754

   /**
    * Cannot execute the current event group in the parallel mode. Encountered event %s, relay-log name %s, position
    * %s which prevents execution of this event group in parallel mode. Reason: %s.
    */
   const val ER_MTS_CANT_PARALLEL: Int = 1755

   /**
    * %s
    */
   const val ER_MTS_INCONSISTENT_DATA: Int = 1756

   /**
    * FULLTEXT index is not supported for partitioned tables.
    */
   const val ER_FULLTEXT_NOT_SUPPORTED_WITH_PARTITIONING: Int = 1757

   /**
    * Invalid condition number
    */
   const val ER_DA_INVALID_CONDITION_NUMBER: Int = 1758

   /**
    * Sending passwords in plain text without SSL/TLS is extremely insecure.
    */
   const val ER_INSECURE_PLAIN_TEXT: Int = 1759

   /**
    * Storing MySQL user name or password information in the master info repository is not secure and is therefore
    * not recommended. Please consider using the USER and PASSWORD connection options for START SLAVE; see the
    * 'START SLAVE Syntax' in the MySQL Manual for more information.
    */
   const val ER_INSECURE_CHANGE_MASTER: Int = 1760

   /**
    * Foreign key constraint for table '%.192s', record '%-.192s' would lead to a duplicate entry in table '%.192s',
    * key '%.192s'
    */
   const val ER_FOREIGN_DUPLICATE_KEY_WITH_CHILD_INFO: Int = 1761

   /**
    * Foreign key constraint for table '%.192s', record '%-.192s' would lead to a duplicate entry in a child table
    */
   const val ER_FOREIGN_DUPLICATE_KEY_WITHOUT_CHILD_INFO: Int = 1762

   /**
    * Setting authentication options is not possible when only the Slave SQL Thread is being started.
    */
   const val ER_SQLTHREAD_WITH_SECURE_SLAVE: Int = 1763

   /**
    * The table does not have FULLTEXT index to support this query
    */
   const val ER_TABLE_HAS_NO_FT: Int = 1764

   /**
    * The system variable %.200s cannot be set in stored functions or triggers.
    */
   const val ER_VARIABLE_NOT_SETTABLE_IN_SF_OR_TRIGGER: Int = 1765

   /**
    * The system variable %.200s cannot be set when there is an ongoing transaction.
    */
   const val ER_VARIABLE_NOT_SETTABLE_IN_TRANSACTION: Int = 1766

   /**
    * The system variable @@SESSION.GTID_NEXT has the value %.200s, which is not listed in @@SESSION.GTID_NEXT_LIST.
    */
   const val ER_GTID_NEXT_IS_NOT_IN_GTID_NEXT_LIST: Int = 1767

   /**
    * The system variable @@SESSION.GTID_NEXT cannot change inside a transaction.
    */
   const val ER_CANT_CHANGE_GTID_NEXT_IN_TRANSACTION_WHEN_GTID_NEXT_LIST_IS_NULL: Int = 1768

   /**
    * The statement 'SET %.200s' cannot invoke a stored function.
    */
   const val ER_SET_STATEMENT_CANNOT_INVOKE_FUNCTION: Int = 1769

   /**
    * The system variable @@SESSION.GTID_NEXT cannot be 'AUTOMATIC' when @@SESSION.GTID_NEXT_LIST is non-NULL.
    */
   const val ER_GTID_NEXT_CANT_BE_AUTOMATIC_IF_GTID_NEXT_LIST_IS_NON_NULL: Int = 1770

   /**
    * Skipping transaction %.200s because it has already been executed and logged.
    */
   const val ER_SKIPPING_LOGGED_TRANSACTION: Int = 1771

   /**
    * Malformed GTID set specification '%.200s'.
    */
   const val ER_MALFORMED_GTID_SET_SPECIFICATION: Int = 1772

   /**
    * Malformed GTID set encoding.
    */
   const val ER_MALFORMED_GTID_SET_ENCODING: Int = 1773

   /**
    * Malformed GTID specification '%.200s'.
    */
   const val ER_MALFORMED_GTID_SPECIFICATION: Int = 1774

   /**
    * Impossible to generate Global Transaction Identifier: the integer component reached the maximal value. Restart
    * the server with a new server_uuid.
    */
   const val ER_GNO_EXHAUSTED: Int = 1775

   /**
    * Parameters MASTER_LOG_FILE, MASTER_LOG_POS, RELAY_LOG_FILE and RELAY_LOG_POS cannot be set when
    * MASTER_AUTO_POSITION is active.
    */
   const val ER_BAD_SLAVE_AUTO_POSITION: Int = 1776

   /**
    * CHANGE MASTER TO MASTER_AUTO_POSITION = 1 can only be executed when @@GLOBAL.GTID_MODE = ON.
    */
   const val ER_AUTO_POSITION_REQUIRES_GTID_MODE_ON: Int = 1777

   /**
    * Cannot execute statements with implicit commit inside a transaction when @@SESSION.GTID_NEXT != AUTOMATIC.
    */
   const val ER_CANT_DO_IMPLICIT_COMMIT_IN_TRX_WHEN_GTID_NEXT_IS_SET: Int = 1778

   /**
    * &#064;&#064;GLOBAL.GTID_MODE = ON or UPGRADE_STEP_2 requires &#064;&#064;GLOBAL.ENFORCE_GTID_CONSISTENCY = 1.
    */
   const val ER_GTID_MODE_2_OR_3_REQUIRES_ENFORCE_GTID_CONSISTENCY_ON: Int = 1779

   /**
    * &#064;&#064;GLOBAL.GTID_MODE = ON or UPGRADE_STEP_1 or UPGRADE_STEP_2 requires --log-bin and --log-slave-updates.
    */
   const val ER_GTID_MODE_REQUIRES_BINLOG: Int = 1780

   /**
    * &#064;&#064;SESSION.GTID_NEXT cannot be set to UUID:NUMBER when &#064;&#064;GLOBAL.GTID_MODE = OFF.
    */
   const val ER_CANT_SET_GTID_NEXT_TO_GTID_WHEN_GTID_MODE_IS_OFF: Int = 1781

   /**
    * &#064;&#064;SESSION.GTID_NEXT cannot be set to ANONYMOUS when &#064;&#064;GLOBAL.GTID_MODE = ON.
    */
   const val ER_CANT_SET_GTID_NEXT_TO_ANONYMOUS_WHEN_GTID_MODE_IS_ON: Int = 1782

   /**
    * &#064;&#064;SESSION.GTID_NEXT_LIST cannot be set to a non-NULL value when &#064;&#064;GLOBAL.GTID_MODE = OFF.
    */
   const val ER_CANT_SET_GTID_NEXT_LIST_TO_NON_NULL_WHEN_GTID_MODE_IS_OFF: Int = 1783

   /**
    * Found a Gtid_log_event or Previous_gtids_log_event when &#064;&#064;GLOBAL.GTID_MODE = OFF.
    */
   const val ER_FOUND_GTID_EVENT_WHEN_GTID_MODE_IS_OFF: Int = 1784

   /**
    * When &#064;&#064;GLOBAL.ENFORCE_GTID_CONSISTENCY = 1, updates to non-transactional tables can only be done in either
    * autocommitted statements or single-statement transactions, and never in the same statement as updates to
    * transactional tables.
    */
   const val ER_GTID_UNSAFE_NON_TRANSACTIONAL_TABLE: Int = 1785

   /**
    * CREATE TABLE ... SELECT is forbidden when &#064;&#064;GLOBAL.ENFORCE_GTID_CONSISTENCY = 1.
    */
   const val ER_GTID_UNSAFE_CREATE_SELECT: Int = 1786

   /**
    * When &#064;&#064;GLOBAL.ENFORCE_GTID_CONSISTENCY = 1, the statements CREATE TEMPORARY TABLE and DROP TEMPORARY TABLE can
    * be executed in a non-transactional context only, and require that AUTOCOMMIT = 1.
    */
   const val ER_GTID_UNSAFE_CREATE_DROP_TEMPORARY_TABLE_IN_TRANSACTION: Int = 1787

   /**
    * The value of &#064;&#064;GLOBAL.GTID_MODE can only change one step at a time: OFF &lt;-&gt; UPGRADE_STEP_1 &lt;-&gt; UPGRADE_STEP_2
    * &lt;-&gt; ON. Also note that this value must be stepped up or down simultaneously on all servers; see the Manual for
    * instructions.
    */
   const val ER_GTID_MODE_CAN_ONLY_CHANGE_ONE_STEP_AT_A_TIME: Int = 1788

   /**
    * The slave is connecting using CHANGE MASTER TO MASTER_AUTO_POSITION = 1, but the master has purged binary logs
    * containing GTIDs that the slave requires.
    */
   const val ER_MASTER_HAS_PURGED_REQUIRED_GTIDS: Int = 1789

   /**
    * &#064;&#064;SESSION.GTID_NEXT cannot be changed by a client that owns a GTID. The client owns %s. Ownership is released
    * on COMMIT or ROLLBACK.
    */
   const val ER_CANT_SET_GTID_NEXT_WHEN_OWNING_GTID: Int = 1790

   /**
    * Unknown EXPLAIN format name: '%s'
    */
   const val ER_UNKNOWN_EXPLAIN_FORMAT: Int = 1791

   /**
    * Cannot execute statement in a READ ONLY transaction.
    */
   const val ER_CANT_EXECUTE_IN_READ_ONLY_TRANSACTION: Int = 1792

   /**
    * Comment for table partition '%-.64s' is too long (max = %lu)
    */
   const val ER_TOO_LONG_TABLE_PARTITION_COMMENT: Int = 1793

   /**
    * Slave is not configured or failed to initialize properly. You must at least set --server-id to enable either a
    * master or a slave. Additional error messages can be found in the MySQL error log.
    */
   const val ER_SLAVE_CONFIGURATION: Int = 1794

   /**
    * InnoDB presently supports one FULLTEXT index creation at a time
    */
   const val ER_INNODB_FT_LIMIT: Int = 1795

   /**
    * Cannot create FULLTEXT index on temporary InnoDB table
    */
   const val ER_INNODB_NO_FT_TEMP_TABLE: Int = 1796

   /**
    * Column '%-.192s' is of wrong type for an InnoDB FULLTEXT index
    */
   const val ER_INNODB_FT_WRONG_DOCID_COLUMN: Int = 1797

   /**
    * Index '%-.192s' is of wrong type for an InnoDB FULLTEXT index
    */
   const val ER_INNODB_FT_WRONG_DOCID_INDEX: Int = 1798

   /**
    * Creating index '%-.192s' required more than 'innodb_online_alter_log_max_size' bytes of modification log.
    * Please try again.
    */
   const val ER_INNODB_ONLINE_LOG_TOO_BIG: Int = 1799

   /**
    * Unknown ALGORITHM '%s'
    */
   const val ER_UNKNOWN_ALTER_ALGORITHM: Int = 1800

   /**
    * Unknown LOCK type '%s'
    */
   const val ER_UNKNOWN_ALTER_LOCK: Int = 1801

   /**
    * CHANGE MASTER cannot be executed when the slave was stopped with an error or killed in MTS mode. Consider
    * using RESET SLAVE or START SLAVE UNTIL.
    */
   const val ER_MTS_CHANGE_MASTER_CANT_RUN_WITH_GAPS: Int = 1802

   /**
    * Cannot recover after SLAVE errored out in parallel execution mode. Additional error messages can be found in
    * the MySQL error log.
    */
   const val ER_MTS_RECOVERY_FAILURE: Int = 1803

   /**
    * Cannot clean up worker info tables. Additional error messages can be found in the MySQL error log.
    */
   const val ER_MTS_RESET_WORKERS: Int = 1804

   /**
    * Column count of %s.%s is wrong. Expected %d, found %d. The table is probably corrupted
    */
   const val ER_COL_COUNT_DOESNT_MATCH_CORRUPTED_V2: Int = 1805

   /**
    * Slave must silently retry current transaction
    */
   const val ER_SLAVE_SILENT_RETRY_TRANSACTION: Int = 1806

   /**
    * There is a foreign key check running on table '%-.192s'. Cannot discard the table.
    */
   const val ER_DISCARD_FK_CHECKS_RUNNING: Int = 1807

   /**
    * Schema mismatch (%s)
    */
   const val ER_TABLE_SCHEMA_MISMATCH: Int = 1808

   /**
    * Table '%-.192s' in system tablespace
    */
   const val ER_TABLE_IN_SYSTEM_TABLESPACE: Int = 1809

   /**
    * IO Read error: (%lu, %s) %s
    */
   const val ER_IO_READ_ERROR: Int = 1810

   /**
    * IO Write error: (%lu, %s) %s
    */
   const val ER_IO_WRITE_ERROR: Int = 1811

   /**
    * Tablespace is missing for table '%-.192s'
    */
   const val ER_TABLESPACE_MISSING: Int = 1812

   /**
    * Tablespace for table '%-.192s' exists. Please DISCARD the tablespace before IMPORT.
    */
   const val ER_TABLESPACE_EXISTS: Int = 1813

   /**
    * Tablespace has been discarded for table '%-.192s'
    */
   const val ER_TABLESPACE_DISCARDED: Int = 1814

   /**
    * Internal error: %s
    */
   const val ER_INTERNAL_ERROR: Int = 1815

   /**
    * ALTER TABLE '%-.192s' IMPORT TABLESPACE failed with error %lu : '%s'
    */
   const val ER_INNODB_IMPORT_ERROR: Int = 1816

   /**
    * Index corrupt: %s
    */
   const val ER_INNODB_INDEX_CORRUPT: Int = 1817

   /**
    * YEAR(%lu) column type is deprecated. Creating YEAR(4) column instead.
    */
   const val ER_INVALID_YEAR_COLUMN_LENGTH: Int = 1818

   /**
    * Your password does not satisfy the current policy requirements
    */
   const val ER_NOT_VALID_PASSWORD: Int = 1819

   /**
    * You must SET PASSWORD before executing this statement
    */
   const val ER_MUST_CHANGE_PASSWORD: Int = 1820

   /**
    * Failed to add the foreign key constaint. Missing index for constraint '%s' in the foreign table '%s'
    */
   const val ER_FK_NO_INDEX_CHILD: Int = 1821

   /**
    * Failed to add the foreign key constaint. Missing index for constraint '%s' in the referenced table '%s'
    */
   const val ER_FK_NO_INDEX_PARENT: Int = 1822

   /**
    * Failed to add the foreign key constraint '%s' to system tables
    */
   const val ER_FK_FAIL_ADD_SYSTEM: Int = 1823

   /**
    * Failed to open the referenced table '%s'
    */
   const val ER_FK_CANNOT_OPEN_PARENT: Int = 1824

   /**
    * Failed to add the foreign key constraint on table '%s'. Incorrect options in FOREIGN KEY constraint '%s'
    */
   const val ER_FK_INCORRECT_OPTION: Int = 1825

   /**
    * Duplicate foreign key constraint name '%s'
    */
   const val ER_FK_DUP_NAME: Int = 1826

   /**
    * The password hash doesn't have the expected format. Check if the correct password algorithm is being used with
    * the PASSWORD() function.
    */
   const val ER_PASSWORD_FORMAT: Int = 1827

   /**
    * Cannot drop column '%-.192s': needed in a foreign key constraint '%-.192s'
    */
   const val ER_FK_COLUMN_CANNOT_DROP: Int = 1828

   /**
    * Cannot drop column '%-.192s': needed in a foreign key constraint '%-.192s' of table '%-.192s'
    */
   const val ER_FK_COLUMN_CANNOT_DROP_CHILD: Int = 1829

   /**
    * Column '%-.192s' cannot be NOT NULL: needed in a foreign key constraint '%-.192s' SET NULL
    */
   const val ER_FK_COLUMN_NOT_NULL: Int = 1830

   /**
    * Duplicate index '%-.64s' defined on the table '%-.64s.%-.64s'. This is deprecated and will be disallowed in a
    * future release.
    */
   const val ER_DUP_INDEX: Int = 1831

   /**
    * Cannot change column '%-.192s': used in a foreign key constraint '%-.192s'
    */
   const val ER_FK_COLUMN_CANNOT_CHANGE: Int = 1832

   /**
    * Cannot change column '%-.192s': used in a foreign key constraint '%-.192s' of table '%-.192s'
    */
   const val ER_FK_COLUMN_CANNOT_CHANGE_CHILD: Int = 1833

   /**
    * Cannot delete rows from table which is parent in a foreign key constraint '%-.192s' of table '%-.192s'
    */
   const val ER_FK_CANNOT_DELETE_PARENT: Int = 1834

   /**
    * Malformed communication packet.
    */
   const val ER_MALFORMED_PACKET: Int = 1835

   /**
    * Running in read-only mode
    */
   const val ER_READ_ONLY_MODE: Int = 1836

   /**
    * When &#064;&#064;SESSION.GTID_NEXT is set to a GTID, you must explicitly set it to a different value after a COMMIT or
    * ROLLBACK. Please check GTID_NEXT variable manual page for detailed explanation. Current &#064;&#064;SESSION.GTID_NEXT is
    * '%s'.
    */
   const val ER_GTID_NEXT_TYPE_UNDEFINED_GROUP: Int = 1837

   /**
    * The system variable %.200s cannot be set in stored procedures.
    */
   const val ER_VARIABLE_NOT_SETTABLE_IN_SP: Int = 1838

   /**
    * &#064;&#064;GLOBAL.GTID_PURGED can only be set when &#064;&#064;GLOBAL.GTID_MODE = ON.
    */
   const val ER_CANT_SET_GTID_PURGED_WHEN_GTID_MODE_IS_OFF: Int = 1839

   /**
    * &#064;&#064;GLOBAL.GTID_PURGED can only be set when &#064;&#064;GLOBAL.GTID_EXECUTED is empty.
    */
   const val ER_CANT_SET_GTID_PURGED_WHEN_GTID_EXECUTED_IS_NOT_EMPTY: Int = 1840

   /**
    * &#064;&#064;GLOBAL.GTID_PURGED can only be set when there are no ongoing transactions (not even in other clients).
    */
   const val ER_CANT_SET_GTID_PURGED_WHEN_OWNED_GTIDS_IS_NOT_EMPTY: Int = 1841

   /**
    * &#064;&#064;GLOBAL.GTID_PURGED was changed from '%s' to '%s'.
    */
   const val ER_GTID_PURGED_WAS_CHANGED: Int = 1842

   /**
    * &#064;&#064;GLOBAL.GTID_EXECUTED was changed from '%s' to '%s'.
    */
   const val ER_GTID_EXECUTED_WAS_CHANGED: Int = 1843

   /**
    * Cannot execute statement: impossible to write to binary log since BINLOG_FORMAT = STATEMENT, and both
    * replicated and non replicated tables are written to.
    */
   const val ER_BINLOG_STMT_MODE_AND_NO_REPL_TABLES: Int = 1844

   /**
    * %s is not supported for this operation. Try %s.
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED: Int = 1845

   /**
    * %s is not supported. Reason: %s. Try %s.
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON: Int = 1846

   /**
    * COPY algorithm requires a lock
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_COPY: Int = 1847

   /**
    * Partition specific operations do not yet support LOCK/ALGORITHM
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_PARTITION: Int = 1848

   /**
    * Columns participating in a foreign key are renamed
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_FK_RENAME: Int = 1849

   /**
    * Cannot change column type INPLACE
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_COLUMN_TYPE: Int = 1850

   /**
    * Adding foreign keys needs foreign_key_checks=OFF
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_FK_CHECK: Int = 1851

   /**
    * Creating unique indexes with IGNORE requires COPY algorithm to remove duplicate rows
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_IGNORE: Int = 1852

   /**
    * Dropping a primary key is not allowed without also adding a new primary key
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_NOPK: Int = 1853

   /**
    * Adding an auto-increment column requires a lock
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_AUTOINC: Int = 1854

   /**
    * Cannot replace hidden FTS_DOC_ID with a user-visible one
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_HIDDEN_FTS: Int = 1855

   /**
    * Cannot drop or rename FTS_DOC_ID
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_CHANGE_FTS: Int = 1856

   /**
    * Fulltext index creation requires a lock
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_FTS: Int = 1857

   /**
    * Sql_slave_skip_counter can not be set when the server is running with @@GLOBAL.GTID_MODE = ON. Instead, for
    * each transaction that you want to skip, generate an empty transaction with the same GTID as the transaction
    */
   const val ER_SQL_SLAVE_SKIP_COUNTER_NOT_SETTABLE_IN_GTID_MODE: Int = 1858

   /**
    * Duplicate entry for key '%-.192s'
    */
   const val ER_DUP_UNKNOWN_IN_INDEX: Int = 1859

   /**
    * Long database name and identifier for object resulted in path length exceeding %d characters. Path: '%s'.
    */
   const val ER_IDENT_CAUSES_TOO_LONG_PATH: Int = 1860

   /**
    * Cannot silently convert NULL values, as required in this SQL_MODE
    */
   const val ER_ALTER_OPERATION_NOT_SUPPORTED_REASON_NOT_NULL: Int = 1861

   /**
    * Your password has expired. To log in you must change it using a client that supports expired passwords.
    */
   const val ER_MUST_CHANGE_PASSWORD_LOGIN: Int = 1862

   /**
    * Found a row in wrong partition %s
    */
   const val ER_ROW_IN_WRONG_PARTITION: Int = 1863

   /**
    * Cannot schedule event %s, relay-log name %s, position %s to Worker thread because its size %lu exceeds %lu of
    * slave_pending_jobs_size_max.
    */
   const val ER_MTS_EVENT_BIGGER_PENDING_JOBS_SIZE_MAX: Int = 1864

   /**
    * Cannot CREATE FULLTEXT INDEX WITH PARSER on InnoDB table
    */
   const val ER_INNODB_NO_FT_USES_PARSER: Int = 1865

   /**
    * The binary log file '%s' is logically corrupted: %s
    */
   const val ER_BINLOG_LOGICAL_CORRUPTION: Int = 1866

   /**
    * File %s was not purged because it was being read by %d thread(s), purged only %d out of %d files.
    */
   const val ER_WARN_PURGE_LOG_IN_USE: Int = 1867

   /**
    * File %s was not purged because it is the active log file.
    */
   const val ER_WARN_PURGE_LOG_IS_ACTIVE: Int = 1868

   /**
    * Auto-increment value in UPDATE conflicts with internally generated values
    */
   const val ER_AUTO_INCREMENT_CONFLICT: Int = 1869

   /**
    * Row events are not logged for %s statements that modify BLACKHOLE tables in row format. Table(s): '%-.192s'
    */
   const val WARN_ON_BLOCKHOLE_IN_RBR: Int = 1870

   /**
    * Slave failed to initialize master info structure from the repository
    */
   const val ER_SLAVE_MI_INIT_REPOSITORY: Int = 1871

   /**
    * Slave failed to initialize relay log info structure from the repository
    */
   const val ER_SLAVE_RLI_INIT_REPOSITORY: Int = 1872

   /**
    * Access denied trying to change to user '%-.48s'@'%-.64s' (using password: %s). Disconnecting.
    */
   const val ER_ACCESS_DENIED_CHANGE_USER_ERROR: Int = 1873

   /**
    * InnoDB is in read only mode.
    */
   const val ER_INNODB_READ_ONLY: Int = 1874

   /**
    * STOP SLAVE command execution is incomplete: Slave SQL thread got the stop signal, thread is busy, SQL thread
    * will stop once the current task is complete.
    */
   const val ER_STOP_SLAVE_SQL_THREAD_TIMEOUT: Int = 1875

   /**
    * STOP SLAVE command execution is incomplete: Slave IO thread got the stop signal, thread is busy, IO thread
    * will stop once the current task is complete.
    */
   const val ER_STOP_SLAVE_IO_THREAD_TIMEOUT: Int = 1876

   /**
    * Operation cannot be performed. The table '%-.64s.%-.64s' is missing, corrupt or contains bad data.
    */
   const val ER_TABLE_CORRUPT: Int = 1877

   /**
    * Temporary file write failure.
    */
   const val ER_TEMP_FILE_WRITE_FAILURE: Int = 1878

   /**
    * Upgrade index name failed, please use create index(alter table) algorithm copy to rebuild index.
    */
   const val ER_INNODB_FT_AUX_NOT_HEX_ID: Int = 1879

   /**
    * TIME/TIMESTAMP/DATETIME columns of old format have been upgraded to the new format.
    */
   const val ER_OLD_TEMPORALS_UPGRADED: Int = 1880

   /**
    * Operation not allowed when innodb_forced_recovery &gt; 0.
    */
   const val ER_INNODB_FORCED_RECOVERY: Int = 1881

   /**
    * The initialization vector supplied to %s is too short. Must be at least %d bytes long
    */
   const val ER_AES_INVALID_IV: Int = 1882

   /**
    * Plugin '%s' cannot be uninstalled now. %s
    */
   const val ER_PLUGIN_CANNOT_BE_UNINSTALLED: Int = 1883

   /**
    * Cannot execute statement because it needs to be written to the binary log as multiple statements, and this is
    * not allowed when @@SESSION.GTID_NEXT == 'UUID:NUMBER'.
    */
   const val ER_GTID_UNSAFE_BINLOG_SPLITTABLE_STATEMENT_AND_GTID_GROUP: Int = 1884

   /**
    * Slave has more GTIDs than the master has, using the master's SERVER_UUID. This may indicate that the end of
    * the binary log was truncated or that the last binary log file was lost, e.g., after a power or disk failure
    * when sync_binlog != 1. The master may or may not have rolled back transactions that were already replicated to
    * the slave. Suggest to replicate any transactions that master has rolled back from slave to master, and/or
    * commit empty transactions on master to account for transactions that have been committed on master but are not
    * included in GTID_EXECUTED.
    */
   const val ER_SLAVE_HAS_MORE_GTIDS_THAN_MASTER: Int = 1885
}
package bop.c.lmdb;

/// Cursor Get operations.
///
/// This is the set of all operations for retrieving data using a cursor.
interface CursorOp {
  /// Position at first key/data item
  int FIRST = 0;

  /// Position at first data item of current key. Only for #DUPSORT
  int FIRST_DUP = 1;

  /// Position at key/data pair. Only for #DUPSORT
  int GET_BOTH = 2;

  /// position at key, nearest data. Only for #DUPSORT
  int GET_BOTH_RANGE = 3;

  /// Return key/data at current cursor position
  int GET_CURRENT = 4;

  /// Return up to a page of duplicate data items from current cursor position. Move cursor to
  /// prepare for #NEXT_MULTIPLE. Only for #DUPFIXED
  int GET_MULTIPLE = 5;

  /** ^ Position at last key/data item */
  int LAST = 6;

  /// Position at last data item of current key. Only for #DUPSORT
  int LAST_DUP = 7;

  /// Position at next data item
  int NEXT = 8;

  /// Position at next data item of current key. Only for #DUPSORT
  int NEXT_DUP = 9;

  /// Return up to a page of duplicate data items from next cursor position. Move cursor to
  /// prepare for #NEXT_MULTIPLE. Only for #DUPFIXED
  int NEXT_MULTIPLE = 10;

  /// Position at first data item of next key
  int NEXT_NODUP = 11;

  /// Position at previous data item
  int PREV = 12;

  /// Position at previous data item of current key. Only for #DUPSORT
  int PREV_DUP = 13;

  /// Position at last data item of previous key
  int PREV_NODUP = 14;

  /// Position at specified key
  int SET = 15;

  /// Position at specified key, return key + data
  int SET_KEY = 16;

  /// Position at first key greater than or equal to specified key.
  int SET_RANGE = 17;

  /// Position at previous page and return up to a page of duplicate data items. Only for
  /// #DUPFIXED
  int PREV_MULTIPLE = 18;
}

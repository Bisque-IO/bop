package bop.c.mdbx;

public class Cursor {
  /// \brief Cursor operations
  /// \ingroup c_cursors
  /// This is the set of all operations for retrieving data using a cursor.
  /// \see mdbx_cursor_get()
  public interface Op {
    /// Position at first key/data item
    int FIRST = 0;

    /// \ref MDBX_DUPSORT -only: Position at first data item of current key.
    int FIRST_DUP = 1;

    /// \ref MDBX_DUPSORT -only: Position at key/data pair.
    int GET_BOTH = 2;

    /// \ref MDBX_DUPSORT -only: Position at given key and at first data greater
    /// than or equal to specified data.
    int GET_BOTH_RANGE = 3;

    /// Return key/data at current cursor position
    int GET_CURRENT = 4;

    /// \ref MDBX_DUPFIXED -only: Return up to a page of duplicate data items
    /// from current cursor position. Move cursor to prepare
    /// for \ref MDBX_NEXT_MULTIPLE. \see MDBX_SEEK_AND_GET_MULTIPLE
    int GET_MULTIPLE = 5;

    /// Position at last key/data item
    int LAST = 6;

    /// \ref MDBX_DUPSORT -only: Position at last data item of current key.
    int LAST_DUP = 7;

    /// Position at next data item
    int NEXT = 8;

    /// \ref MDBX_DUPSORT -only: Position at next data item of current key.
    int NEXT_DUP = 9;

    /// \ref MDBX_DUPFIXED -only: Return up to a page of duplicate data items
    /// from next cursor position. Move cursor to prepare for `MDBX_NEXT_MULTIPLE`.
    /// \see MDBX_SEEK_AND_GET_MULTIPLE \see MDBX_GET_MULTIPLE
    int NEXT_MULTIPLE = 10;

    /// Position at first data item of next key
    int NEXT_NODUP = 11;

    /// Position at previous data item
    int PREV = 12;

    /// \ref MDBX_DUPSORT -only: Position at previous data item of current key.
    int PREV_DUP = 13;

    /// Position at last data item of previous key
    int PREV_NODUP = 14;

    /// Position at specified key
    int SET = 15;

    /// Position at specified key, return both key and data
    int SET_KEY = 16;

    /// Position at first key greater than or equal to specified key.
    int SET_RANGE = 17;

    /// \ref MDBX_DUPFIXED -only: Position at previous page and return up to
    /// a page of duplicate data items.
    /// \see MDBX_SEEK_AND_GET_MULTIPLE \see MDBX_GET_MULTIPLE
    int PREV_MULTIPLE = 18;

    /// Positions cursor at first key-value pair greater than or equal to
    /// specified, return both key and data, and the return code depends on whether
    /// a exact match.
    ///
    /// For non DUPSORT-ed collections this work the same to \ref MDBX_SET_RANGE,
    /// but returns \ref MDBX_SUCCESS if key found exactly or
    /// \ref MDBX_RESULT_TRUE if greater key was found.
    ///
    /// For DUPSORT-ed a data value is taken into account for duplicates,
    /// i.e. for a pairs/tuples of a key and an each data value of duplicates.
    /// Returns \ref MDBX_SUCCESS if key-value pair found exactly or
    /// \ref MDBX_RESULT_TRUE if the next pair was returned.
    int SET_LOWERBOUND = 19;

    /// Positions cursor at first key-value pair greater than specified,
    /// return both key and data, and the return code depends on whether a
    /// upper-bound was found.
    ///
    /// For non DUPSORT-ed collections this work like \ref MDBX_SET_RANGE,
    /// but returns \ref MDBX_SUCCESS if the greater key was found or
    /// \ref MDBX_NOTFOUND otherwise.
    ///
    /// For DUPSORT-ed a data value is taken into account for duplicates,
    /// i.e. for a pairs/tuples of a key and an each data value of duplicates.
    /// Returns \ref MDBX_SUCCESS if the greater pair was returned or
    /// \ref MDBX_NOTFOUND otherwise.
    int SET_UPPERBOUND = 20;

    /** Doubtless cursor positioning at a specified key. */
    int TO_KEY_LESSER_THAN = 21;

    int TO_KEY_LESSER_OR_EQUAL = 22;
    int TO_KEY_EQUAL = 23;
    int TO_KEY_GREATER_OR_EQUAL = 24;
    int TO_KEY_GREATER_THAN = 25;

    /** Doubtless cursor positioning at a specified key-value pair for dupsort/multi-value hives. */
    int TO_EXACT_KEY_VALUE_LESSER_THAN = 26;

    int TO_EXACT_KEY_VALUE_LESSER_OR_EQUAL = 27;
    int TO_EXACT_KEY_VALUE_EQUAL = 28;
    int TO_EXACT_KEY_VALUE_GREATER_OR_EQUAL = 29;
    int TO_EXACT_KEY_VALUE_GREATER_THAN = 30;

    /** Doubtless cursor positioning at a specified key-value pair for dupsort/multi-value hives. */
    int TO_PAIR_LESSER_THAN = 31;

    int TO_PAIR_LESSER_OR_EQUAL = 32;
    int TO_PAIR_EQUAL = 33;
    int TO_PAIR_GREATER_OR_EQUAL = 34;
    int TO_PAIR_GREATER_THAN = 35;

    /// \ref MDBX_DUPFIXED -only: Seek to given key and return up to a page of
    /// duplicate data items from current cursor position. Move cursor to prepare
    /// for \ref MDBX_NEXT_MULTIPLE. \see MDBX_GET_MULTIPLE
    int SEEK_AND_GET_MULTIPLE = 36;
  }
}

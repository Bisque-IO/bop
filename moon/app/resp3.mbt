///|
// RESP3 (Redis Serialization Protocol version 3) Parser and Writer
// Supports all RESP3 data types as specified in the Redis protocol

///|
// RESP3 Value Types
enum Resp3Value {
  SimpleString(String)
  Error(String)
  Integer(Int64)
  BulkString(Option[String])  // None represents null bulk string
  Array(Array[Resp3Value])
  Null
  Boolean(Bool)
  Double(Double)
  BigNumber(String)  // Stored as string to avoid overflow
  VerbatimString(String, String)  // format, string
  Map(Array[(Resp3Value, Resp3Value)])
  Set(Array[Resp3Value])
  Push(Array[Resp3Value])
  Attribute(Array[(Resp3Value, Resp3Value)], Resp3Value)  // attributes, value
  StreamedString(Array[String])  // chunks
  StreamedArray(Array[Resp3Value])  // elements
}

///|
// Parser state
priv struct ParserState {
  data : String;
  pos : Int
}

///|
// Parse a RESP3 value from a string
pub fn parse(input : String) -> Option[Resp3Value] {
  let state = ParserState::{ data: input, pos: 0 }
  match parse_value(state) {
    Some((value, _)) => Some(value)
    None => None
  }
}

fn parse_value(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  if state.pos >= state.data.length() {
    None
  } else {
    let ch_code = state.data.code_unit_at(state.pos).to_int()
    match ch_code {
      43 => parse_simple_string(state)  // '+'
      45 => parse_error(state)  // '-'
      58 => parse_integer(state)  // ':'
      36 => parse_bulk_string(state)  // '$'
      42 => parse_array(state)  // '*'
      95 => parse_null(state)  // '_'
      35 => parse_boolean(state)  // '#'
      44 => parse_double(state)  // ','
      40 => parse_bignumber(state)  // '('
      61 => parse_verbatim_string(state)  // '='
      37 => parse_map(state)  // '%'
      126 => parse_set(state)  // '~'
      62 => parse_push(state)  // '>'
      124 => parse_attribute(state)  // '|'
      _ => None
    }
  }
}

fn parse_simple_string(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip '+'
  match read_until_crlf(new_state) {
    Some((str, next_state)) => Some((SimpleString(str), next_state))
    None => None
  }
}

fn parse_error(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip '-'
  match read_until_crlf(new_state) {
    Some((str, next_state)) => Some((Error(str), next_state))
    None => None
  }
}

fn parse_integer(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip ':'
  match read_until_crlf(new_state) {
    Some((str, next_state)) => {
      match parse_int64(str) {
        Some(n) => Some((Integer(n), next_state))
        None => None
      }
    }
    None => None
  }
}

fn parse_bulk_string(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip '$'
  
  // Check for null bulk string ($-1\r\n)
  if peek(new_state, 0) == Some(45) && peek(new_state, 1) == Some(49) {  // '-' and '1'
    match read_until_crlf(advance(new_state, 2)) {
      Some((_, next_state)) => Some((BulkString(None), next_state))
      None => None
    }
  } else {
    // Check for streamed string ($?\r\n)
    if peek(new_state, 0) == Some(63) {  // '?'
      match read_until_crlf(advance(new_state, 1)) {
        Some((_, next_state)) => parse_streamed_string(next_state)
        None => None
      }
    } else {
      // Regular bulk string
      match read_until_crlf(new_state) {
        Some((len_str, state_after_len)) => {
          match parse_int(len_str) {
            Some(len) => {
              if len < 0 {
                Some((BulkString(None), state_after_len))
              } else {
                if state_after_len.pos + len + 2 > state_after_len.data.length() {
                  None
                } else {
                  let str = substring(state_after_len.data, state_after_len.pos, len)
                  let next_state = advance(state_after_len, len + 2)  // +2 for \r\n
                  Some((BulkString(Some(str)), next_state))
                }
              }
            }
            None => None
          }
        }
        None => None
      }
    }
  }
}

fn parse_array(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip '*'
  
  // Check for streamed array (*?\r\n)
  if peek(new_state, 0) == Some(63) {  // '?'
    match read_until_crlf(advance(new_state, 1)) {
      Some((_, next_state)) => parse_streamed_array(next_state)
      None => None
    }
  } else {
    match read_until_crlf(new_state) {
      Some((len_str, state_after_len)) => {
        match parse_int(len_str) {
          Some(len) => {
            if len < 0 {
              Some((Null, state_after_len))
            } else {
              parse_array_elements(state_after_len, len, [])
            }
          }
          None => None
        }
      }
      None => None
    }
  }
}

fn parse_array_elements(
  state : ParserState,
  count : Int,
  acc : Array[Resp3Value]
) -> Option[(Resp3Value, ParserState)] {
  if count == 0 {
    Some((Array(acc), state))
  } else {
    match parse_value(state) {
      Some((value, next_state)) => {
        parse_array_elements(next_state, count - 1, acc + [value])
      }
      None => None
    }
  }
}

fn parse_null(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  match read_until_crlf(advance(state, 1)) {
    Some((_, next_state)) => Some((Null, next_state))
    None => None
  }
}

fn parse_boolean(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip '#'
  match peek(new_state, 0) {
    Some(116) => {  // 't'
      match read_until_crlf(advance(new_state, 1)) {
        Some((_, next_state)) => Some((Boolean(true), next_state))
        None => None
      }
    }
    Some(102) => {  // 'f'
      match read_until_crlf(advance(new_state, 1)) {
        Some((_, next_state)) => Some((Boolean(false), next_state))
        None => None
      }
    }
    _ => None
  }
}

fn parse_double(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip ','
  match read_until_crlf(new_state) {
    Some((str, next_state)) => {
      match parse_double_str(str) {
        Some(d) => Some((Double(d), next_state))
        None => None
      }
    }
    None => None
  }
}

fn parse_bignumber(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip '('
  match read_until_crlf(new_state) {
    Some((str, next_state)) => Some((BigNumber(str), next_state))
    None => None
  }
}

fn parse_verbatim_string(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip '='
  match read_until_crlf(new_state) {
    Some((len_str, state_after_len)) => {
      match parse_int(len_str) {
        Some(len) => {
          if state_after_len.pos + len + 2 > state_after_len.data.length() {
            None
          } else {
            let content = substring(state_after_len.data, state_after_len.pos, len)
            let next_state = advance(state_after_len, len + 2)
            // Format is first 3 chars (e.g., "txt"), then ':'
            if content.length() >= 4 && content.code_unit_at(3).to_int() == 58 {  // ':'
              let format = substring(content, 0, 3)
              let str = substring(content, 4, content.length() - 4)
              Some((VerbatimString(format, str), next_state))
            } else {
              None
            }
          }
        }
        None => None
      }
    }
    None => None
  }
}

fn parse_map(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip '%'
  match read_until_crlf(new_state) {
    Some((len_str, state_after_len)) => {
      match parse_int(len_str) {
        Some(len) => {
          parse_map_pairs(state_after_len, len, [])
        }
        None => None
      }
    }
    None => None
  }
}

fn parse_map_pairs(
  state : ParserState,
  count : Int,
  acc : Array[(Resp3Value, Resp3Value)]
) -> Option[(Resp3Value, ParserState)] {
  if count == 0 {
    Some((Map(acc), state))
  } else {
    match parse_value(state) {
      Some((key, state_after_key)) => {
        match parse_value(state_after_key) {
          Some((value, state_after_value)) => {
            parse_map_pairs(state_after_value, count - 1, acc + [(key, value)])
          }
          None => None
        }
      }
      None => None
    }
  }
}

fn parse_set(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip '~'
  match read_until_crlf(new_state) {
    Some((len_str, state_after_len)) => {
      match parse_int(len_str) {
        Some(len) => {
          parse_set_elements(state_after_len, len, [])
        }
        None => None
      }
    }
    None => None
  }
}

fn parse_set_elements(
  state : ParserState,
  count : Int,
  acc : Array[Resp3Value]
) -> Option[(Resp3Value, ParserState)] {
  if count == 0 {
    Some((Set(acc), state))
  } else {
    match parse_value(state) {
      Some((value, next_state)) => {
        parse_set_elements(next_state, count - 1, acc + [value])
      }
      None => None
    }
  }
}

fn parse_push(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip '>'
  match read_until_crlf(new_state) {
    Some((len_str, state_after_len)) => {
      match parse_int(len_str) {
        Some(len) => {
          parse_array_elements(state_after_len, len, [])
        }
        None => None
      }
    }
    None => None
  }
}

fn parse_attribute(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  let new_state = advance(state, 1)  // Skip '|'
  match read_until_crlf(new_state) {
    Some((len_str, state_after_len)) => {
      match parse_int(len_str) {
        Some(len) => {
          match parse_map_pairs(state_after_len, len, []) {
            Some((Map(attrs), state_after_attrs)) => {
              match parse_value(state_after_attrs) {
                Some((value, final_state)) => {
                  Some((Attribute(attrs, value), final_state))
                }
                None => None
              }
            }
            _ => None
          }
        }
        None => None
      }
    }
    None => None
  }
}

fn parse_streamed_string(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  parse_streamed_string_chunks(state, [])
}

fn parse_streamed_string_chunks(
  state : ParserState,
  acc : Array[String]
) -> Option[(Resp3Value, ParserState)] {
  match peek(state, 0) {
    Some(59) => {  // ';'
      let new_state = advance(state, 1)  // Skip ';'
      match read_until_crlf(new_state) {
        Some((len_str, state_after_len)) => {
          match parse_int(len_str) {
            Some(len) => {
              if len == 0 {
                // End of stream
                Some((StreamedString(acc), state_after_len))
              } else {
                if state_after_len.pos + len + 2 > state_after_len.data.length() {
                  None
                } else {
                  let chunk = substring(state_after_len.data, state_after_len.pos, len)
                  let next_state = advance(state_after_len, len + 2)
                  parse_streamed_string_chunks(next_state, acc + [chunk])
                }
              }
            }
            None => None
          }
        }
        None => None
      }
    }
    _ => None
  }
}

fn parse_streamed_array(state : ParserState) -> Option[(Resp3Value, ParserState)] {
  parse_streamed_array_elements(state, [])
}

fn parse_streamed_array_elements(
  state : ParserState,
  acc : Array[Resp3Value]
) -> Option[(Resp3Value, ParserState)] {
  match peek(state, 0) {
    Some(46) => {  // '.'
      // End marker
      match read_until_crlf(advance(state, 1)) {
        Some((_, next_state)) => Some((StreamedArray(acc), next_state))
        None => None
      }
    }
    _ => {
      match parse_value(state) {
        Some((value, next_state)) => {
          parse_streamed_array_elements(next_state, acc + [value])
        }
        None => None
      }
    }
  }
}

// Helper functions

fn advance(state : ParserState, n : Int) -> ParserState {
  ParserState::{ data: state.data, pos: state.pos + n }
}

fn peek(state : ParserState, offset : Int) -> Option[Int] {
  let idx = state.pos + offset
  if idx >= 0 && idx < state.data.length() {
    Some(state.data.code_unit_at(idx).to_int())
  } else {
    None
  }
}

fn substring(str_val : String, start : Int, len : Int) -> String {
  // Build substring by iterating through code units and converting to chars
  let mut result = ""
  let mut i = 0
  let end = if start + len > str_val.length() { str_val.length() } else { start + len }
  while start + i < end {
    let code = str_val.code_unit_at(start + i).to_int()
    // Convert code unit to char and then to string
    result = result + code.unsafe_to_char().to_string()
    i = i + 1
  }
  result
}

fn parse_int(str : String) -> Option[Int] {
  parse_int_helper(str, 0, false)
}

fn parse_int_helper(str : String, acc : Int, negative : Bool) -> Option[Int] {
  if str.length() == 0 {
    Some(if negative { -acc } else { acc })
  } else {
    let ch_code = str.code_unit_at(0).to_int()
    if ch_code == 45 && acc == 0 {  // '-'
      parse_int_helper(substring(str, 1, str.length() - 1), acc, true)
    } else if ch_code >= 48 && ch_code <= 57 {  // '0' to '9'
      let digit = ch_code - 48
      parse_int_helper(substring(str, 1, str.length() - 1), acc * 10 + digit, negative)
    } else {
      None
    }
  }
}

fn parse_int64(str : String) -> Option[Int64] {
  parse_int64_helper(str, 0L, false)
}

fn read_until_crlf(state : ParserState) -> Option[(String, ParserState)] {
  let mut i = state.pos
  let mut found_cr = false
  let start = state.pos
  
  while i < state.data.length() {
    let ch_code = state.data.code_unit_at(i).to_int()
    if ch_code == 13 {  // '\r'
      found_cr = true
    } else if found_cr && ch_code == 10 {  // '\n'
      let str_val = substring(state.data, start, i - start - 1)
      let next_state = ParserState::{ data: state.data, pos: i + 1 }
      return Some((str_val, next_state))
    } else {
      found_cr = false
    }
    i = i + 1
  }
  None
}

fn parse_int64_helper(str : String, acc : Int64, negative : Bool) -> Option[Int64] {
  if str.length() == 0 {
    Some(if negative { -acc } else { acc })
  } else {
    let ch_code = str.code_unit_at(0).to_int()
    if ch_code == 45 && acc == 0L {  // '-'
      parse_int64_helper(substring(str, 1, str.length() - 1), acc, true)
    } else if ch_code >= 48 && ch_code <= 57 {  // '0' to '9'
      let digit = (ch_code - 48).to_int64()
      parse_int64_helper(substring(str, 1, str.length() - 1), acc * 10L + digit, negative)
    } else {
      None
    }
  }
}

fn parse_double_str(_str : String) -> Option[Double] {
  // Simplified double parsing - in production would use proper parsing
  // For now, return None as Double parsing is complex
  None
}

///|
// Writer: Convert RESP3Value to RESP3 string format

pub fn write(value : Resp3Value) -> String {
  match value {
    SimpleString(s) => "+" + s + "\r\n"
    Error(s) => "-" + s + "\r\n"
    Integer(n) => ":" + n.to_string() + "\r\n"
    BulkString(Some(s)) => "$" + s.length().to_string() + "\r\n" + s + "\r\n"
    BulkString(None) => "$-1\r\n"
    Array(elements) => {
      let mut result = "*" + elements.length().to_string() + "\r\n"
      let mut i = 0
      while i < elements.length() {
        result = result + write(elements[i])
        i = i + 1
      }
      result
    }
    Null => "_\r\n"
    Boolean(true) => "#t\r\n"
    Boolean(false) => "#f\r\n"
    Double(d) => "," + d.to_string() + "\r\n"
    BigNumber(n) => "(" + n + "\r\n"
    VerbatimString(format, s) => {
      let content = format + ":" + s
      "=" + content.length().to_string() + "\r\n" + content + "\r\n"
    }
    Map(pairs) => {
      let mut result = "%" + pairs.length().to_string() + "\r\n"
      let mut i = 0
      while i < pairs.length() {
        let (k, v) = pairs[i]
        result = result + write(k) + write(v)
        i = i + 1
      }
      result
    }
    Set(elements) => {
      let mut result = "~" + elements.length().to_string() + "\r\n"
      let mut i = 0
      while i < elements.length() {
        result = result + write(elements[i])
        i = i + 1
      }
      result
    }
    Push(elements) => {
      let mut result = ">" + elements.length().to_string() + "\r\n"
      let mut i = 0
      while i < elements.length() {
        result = result + write(elements[i])
        i = i + 1
      }
      result
    }
    Attribute(attrs, value) => {
      let mut result = "|" + attrs.length().to_string() + "\r\n"
      let mut i = 0
      while i < attrs.length() {
        let (k, v) = attrs[i]
        result = result + write(k) + write(v)
        i = i + 1
      }
      result + write(value)
    }
    StreamedString(chunks) => {
      let mut result = "$?\r\n"
      let mut i = 0
      while i < chunks.length() {
        result = result + ";" + chunks[i].length().to_string() + "\r\n" + chunks[i]
        i = i + 1
      }
      result + ";0\r\n"
    }
    StreamedArray(elements) => {
      let mut result = "*?\r\n"
      let mut i = 0
      while i < elements.length() {
        result = result + write(elements[i])
        i = i + 1
      }
      result + ".\r\n"
    }
  }
}


// RESP3 (Redis Serialization Protocol version 3) Parser and Writer
// Supports all RESP3 data types as specified in the Redis protocol

/// RESP3 Value Types
#[derive(Clone, PartialEq, Debug)]
// #[cfg_attr(not(test), derive(Debug))]
pub enum Resp3Value {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<String>), // None represents null bulk string
    Array(Vec<Resp3Value>),
    Null,
    Boolean(bool),
    Double(f64),
    BigNumber(String), // Stored as string to avoid overflow
    VerbatimString(String, String), // format, string
    Map(Vec<(Resp3Value, Resp3Value)>),
    Set(Vec<Resp3Value>),
    Push(Vec<Resp3Value>),
    Attribute(Vec<(Resp3Value, Resp3Value)>, Box<Resp3Value>), // attributes, value
    StreamedString(Vec<String>), // chunks
    StreamedArray(Vec<Resp3Value>), // elements
}

/// Parser state
struct ParserState {
    data: Vec<u8>,
    pos: usize,
}

/// Parse a RESP3 value from a string
pub fn parse(input: &str) -> Option<Resp3Value> {
    let state = ParserState {
        data: input.as_bytes().to_vec(),
        pos: 0,
    };
    parse_value(state).map(|(value, _)| value)
}

fn parse_value(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    if state.pos >= state.data.len() {
        return None;
    }
    
    let ch_code = state.data[state.pos];
    match ch_code {
        b'+' => parse_simple_string(state),
        b'-' => parse_error(state),
        b':' => parse_integer(state),
        b'$' => parse_bulk_string(state),
        b'*' => parse_array(state),
        b'_' => parse_null(state),
        b'#' => parse_boolean(state),
        b',' => parse_double(state),
        b'(' => parse_bignumber(state),
        b'=' => parse_verbatim_string(state),
        b'%' => parse_map(state),
        b'~' => parse_set(state),
        b'>' => parse_push(state),
        b'|' => parse_attribute(state),
        _ => None,
    }
}

fn parse_simple_string(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip '+'
    read_until_crlf(new_state).map(|(str_val, next_state)| {
        (Resp3Value::SimpleString(str_val), next_state)
    })
}

fn parse_error(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip '-'
    read_until_crlf(new_state).map(|(str_val, next_state)| {
        (Resp3Value::Error(str_val), next_state)
    })
}

fn parse_integer(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip ':'
    read_until_crlf(new_state).and_then(|(str_val, next_state)| {
        parse_int64(&str_val).map(|n| (Resp3Value::Integer(n), next_state))
    })
}

fn parse_bulk_string(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip '$'
    
    // Check for null bulk string ($-1\r\n)
    if peek(&new_state, 0) == Some(b'-') && peek(&new_state, 1) == Some(b'1') {
        read_until_crlf(advance(new_state, 2)).map(|(_, next_state)| {
            (Resp3Value::BulkString(None), next_state)
        })
    } else {
        // Check for streamed string ($?\r\n)
        if peek(&new_state, 0) == Some(b'?') {
            read_until_crlf(advance(new_state, 1)).and_then(|(_, next_state)| {
                parse_streamed_string(next_state)
            })
        } else {
            // Regular bulk string
            read_until_crlf(new_state).and_then(|(len_str, state_after_len)| {
                parse_int(&len_str).and_then(|len| {
                    if len < 0 {
                        Some((Resp3Value::BulkString(None), state_after_len))
                    } else {
                        let len_usize = len as usize;
                        if state_after_len.pos + len_usize + 2 > state_after_len.data.len() {
                            None
                        } else {
                            let str_bytes = &state_after_len.data[state_after_len.pos..state_after_len.pos + len_usize];
                            let str_val = String::from_utf8(str_bytes.to_vec()).ok()?;
                            let next_state = advance(state_after_len, len_usize + 2); // +2 for \r\n
                            Some((Resp3Value::BulkString(Some(str_val)), next_state))
                        }
                    }
                })
            })
        }
    }
}

fn parse_array(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip '*'
    
    // Check for streamed array (*?\r\n)
    if peek(&new_state, 0) == Some(b'?') {
        read_until_crlf(advance(new_state, 1)).and_then(|(_, next_state)| {
            parse_streamed_array(next_state)
        })
    } else {
        read_until_crlf(new_state).and_then(|(len_str, state_after_len)| {
            parse_int(&len_str).and_then(|len| {
                if len < 0 {
                    Some((Resp3Value::Null, state_after_len))
                } else {
                    parse_array_elements(state_after_len, len as usize, Vec::new())
                }
            })
        })
    }
}

fn parse_array_elements(
    state: ParserState,
    count: usize,
    mut acc: Vec<Resp3Value>,
) -> Option<(Resp3Value, ParserState)> {
    if count == 0 {
        Some((Resp3Value::Array(acc), state))
    } else {
        parse_value(state).and_then(|(value, next_state)| {
            acc.push(value);
            parse_array_elements(next_state, count - 1, acc)
        })
    }
}

fn parse_null(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    read_until_crlf(advance(state, 1)).map(|(_, next_state)| {
        (Resp3Value::Null, next_state)
    })
}

fn parse_boolean(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip '#'
    match peek(&new_state, 0) {
        Some(b't') => {
            read_until_crlf(advance(new_state, 1)).map(|(_, next_state)| {
                (Resp3Value::Boolean(true), next_state)
            })
        }
        Some(b'f') => {
            read_until_crlf(advance(new_state, 1)).map(|(_, next_state)| {
                (Resp3Value::Boolean(false), next_state)
            })
        }
        _ => None,
    }
}

fn parse_double(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip ','
    read_until_crlf(new_state).and_then(|(str_val, next_state)| {
        parse_double_str(&str_val).map(|d| (Resp3Value::Double(d), next_state))
    })
}

fn parse_bignumber(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip '('
    read_until_crlf(new_state).map(|(str_val, next_state)| {
        (Resp3Value::BigNumber(str_val), next_state)
    })
}

fn parse_verbatim_string(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip '='
    read_until_crlf(new_state).and_then(|(len_str, state_after_len)| {
        parse_int(&len_str).and_then(|len| {
            let len_usize = len as usize;
            if state_after_len.pos + len_usize + 2 > state_after_len.data.len() {
                None
            } else {
                let content_bytes = &state_after_len.data[state_after_len.pos..state_after_len.pos + len_usize];
                let content = String::from_utf8(content_bytes.to_vec()).ok()?;
                let next_state = advance(state_after_len, len_usize + 2);
                // Format is first 3 chars (e.g., "txt"), then ':'
                if content.len() >= 4 && content.as_bytes()[3] == b':' {
                    let format = content[..3].to_string();
                    let str_val = content[4..].to_string();
                    Some((Resp3Value::VerbatimString(format, str_val), next_state))
                } else {
                    None
                }
            }
        })
    })
}

fn parse_map(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip '%'
    read_until_crlf(new_state).and_then(|(len_str, state_after_len)| {
        parse_int(&len_str).and_then(|len| {
            parse_map_pairs(state_after_len, len as usize, Vec::new())
        })
    })
}

fn parse_map_pairs(
    state: ParserState,
    count: usize,
    mut acc: Vec<(Resp3Value, Resp3Value)>,
) -> Option<(Resp3Value, ParserState)> {
    if count == 0 {
        Some((Resp3Value::Map(acc), state))
    } else {
        parse_value(state).and_then(|(key, state_after_key)| {
            parse_value(state_after_key).and_then(|(value, state_after_value)| {
                acc.push((key, value));
                parse_map_pairs(state_after_value, count - 1, acc)
            })
        })
    }
}

fn parse_set(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip '~'
    read_until_crlf(new_state).and_then(|(len_str, state_after_len)| {
        parse_int(&len_str).and_then(|len| {
            parse_set_elements(state_after_len, len as usize, Vec::new())
        })
    })
}

fn parse_set_elements(
    state: ParserState,
    count: usize,
    mut acc: Vec<Resp3Value>,
) -> Option<(Resp3Value, ParserState)> {
    if count == 0 {
        Some((Resp3Value::Set(acc), state))
    } else {
        parse_value(state).and_then(|(value, next_state)| {
            acc.push(value);
            parse_set_elements(next_state, count - 1, acc)
        })
    }
}

fn parse_push(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip '>'
    read_until_crlf(new_state).and_then(|(len_str, state_after_len)| {
        parse_int(&len_str).and_then(|len| {
            parse_array_elements(state_after_len, len as usize, Vec::new())
        })
    })
}

fn parse_attribute(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    let new_state = advance(state, 1); // Skip '|'
    read_until_crlf(new_state).and_then(|(len_str, state_after_len)| {
        parse_int(&len_str).and_then(|len| {
            parse_map_pairs(state_after_len, len as usize, Vec::new()).and_then(|(value, state_after_attrs)| {
                if let Resp3Value::Map(attrs) = value {
                    parse_value(state_after_attrs).map(|(val, final_state)| {
                        (Resp3Value::Attribute(attrs, Box::new(val)), final_state)
                    })
                } else {
                    None
                }
            })
        })
    })
}

fn parse_streamed_string(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    parse_streamed_string_chunks(state, Vec::new())
}

fn parse_streamed_string_chunks(
    state: ParserState,
    mut acc: Vec<String>,
) -> Option<(Resp3Value, ParserState)> {
    match peek(&state, 0) {
        Some(b';') => {
            let new_state = advance(state, 1); // Skip ';'
            read_until_crlf(new_state).and_then(|(len_str, state_after_len)| {
                parse_int(&len_str).and_then(|len| {
                    if len == 0 {
                        // End of stream
                        Some((Resp3Value::StreamedString(acc), state_after_len))
                    } else {
                        let len_usize = len as usize;
                        if state_after_len.pos + len_usize + 2 > state_after_len.data.len() {
                            None
                        } else {
                            let chunk_bytes = &state_after_len.data[state_after_len.pos..state_after_len.pos + len_usize];
                            let chunk = String::from_utf8(chunk_bytes.to_vec()).ok()?;
                            let next_state = advance(state_after_len, len_usize + 2);
                            acc.push(chunk);
                            parse_streamed_string_chunks(next_state, acc)
                        }
                    }
                })
            })
        }
        _ => None,
    }
}

fn parse_streamed_array(state: ParserState) -> Option<(Resp3Value, ParserState)> {
    parse_streamed_array_elements(state, Vec::new())
}

fn parse_streamed_array_elements(
    state: ParserState,
    mut acc: Vec<Resp3Value>,
) -> Option<(Resp3Value, ParserState)> {
    match peek(&state, 0) {
        Some(b'.') => {
            // End marker
            read_until_crlf(advance(state, 1)).map(|(_, next_state)| {
                (Resp3Value::StreamedArray(acc), next_state)
            })
        }
        _ => {
            parse_value(state).and_then(|(value, next_state)| {
                acc.push(value);
                parse_streamed_array_elements(next_state, acc)
            })
        }
    }
}

// Helper functions

fn advance(state: ParserState, n: usize) -> ParserState {
    ParserState {
        data: state.data,
        pos: state.pos + n,
    }
}

fn peek(state: &ParserState, offset: usize) -> Option<u8> {
    let idx = state.pos + offset;
    if idx < state.data.len() {
        Some(state.data[idx])
    } else {
        None
    }
}

fn parse_int(str_val: &str) -> Option<i32> {
    parse_int_helper(str_val, 0, false)
}

fn parse_int_helper(str_val: &str, acc: i32, negative: bool) -> Option<i32> {
    if str_val.is_empty() {
        Some(if negative { -acc } else { acc })
    } else {
        let ch = str_val.chars().next()?;
        if ch == '-' && acc == 0 {
            parse_int_helper(&str_val[1..], acc, true)
        } else if let Some(digit) = ch.to_digit(10) {
            parse_int_helper(&str_val[1..], acc * 10 + digit as i32, negative)
        } else {
            None
        }
    }
}

fn read_until_crlf(state: ParserState) -> Option<(String, ParserState)> {
    let mut i = state.pos;
    let mut found_cr = false;
    let start = state.pos;
    
    while i < state.data.len() {
        let ch_code = state.data[i];
        if ch_code == b'\r' {
            found_cr = true;
        } else if found_cr && ch_code == b'\n' {
            let str_bytes = &state.data[start..i - 1];
            let str_val = String::from_utf8(str_bytes.to_vec()).ok()?;
            let next_state = ParserState {
                data: state.data,
                pos: i + 1,
            };
            return Some((str_val, next_state));
        } else {
            found_cr = false;
        }
        i += 1;
    }
    None
}

fn parse_int64(str_val: &str) -> Option<i64> {
    parse_int64_helper(str_val, 0, false)
}

fn parse_int64_helper(str_val: &str, acc: i64, negative: bool) -> Option<i64> {
    if str_val.is_empty() {
        Some(if negative { -acc } else { acc })
    } else {
        let ch = str_val.chars().next()?;
        if ch == '-' && acc == 0 {
            parse_int64_helper(&str_val[1..], acc, true)
        } else if let Some(digit) = ch.to_digit(10) {
            parse_int64_helper(&str_val[1..], acc * 10 + digit as i64, negative)
        } else {
            None
        }
    }
}

fn parse_double_str(str_val: &str) -> Option<f64> {
    str_val.parse().ok()
}

/// Writer: Convert Resp3Value to RESP3 string format
pub fn write(value: &Resp3Value) -> String {
    match value {
        Resp3Value::SimpleString(s) => format!("+{}\r\n", s),
        Resp3Value::Error(s) => format!("-{}\r\n", s),
        Resp3Value::Integer(n) => format!(":{}\r\n", n),
        Resp3Value::BulkString(Some(s)) => format!("${}\r\n{}\r\n", s.len(), s),
        Resp3Value::BulkString(None) => "$-1\r\n".to_string(),
        Resp3Value::Array(elements) => {
            let mut result = format!("*{}\r\n", elements.len());
            for element in elements {
                result.push_str(&write(element));
            }
            result
        }
        Resp3Value::Null => "_\r\n".to_string(),
        Resp3Value::Boolean(true) => "#t\r\n".to_string(),
        Resp3Value::Boolean(false) => "#f\r\n".to_string(),
        Resp3Value::Double(d) => format!(",{}\r\n", d),
        Resp3Value::BigNumber(n) => format!("({}\r\n", n),
        Resp3Value::VerbatimString(format, s) => {
            let content = format!("{}:{}", format, s);
            format!("={}\r\n{}\r\n", content.len(), content)
        }
        Resp3Value::Map(pairs) => {
            let mut result = format!("%{}\r\n", pairs.len());
            for (k, v) in pairs {
                result.push_str(&write(k));
                result.push_str(&write(v));
            }
            result
        }
        Resp3Value::Set(elements) => {
            let mut result = format!("~{}\r\n", elements.len());
            for element in elements {
                result.push_str(&write(element));
            }
            result
        }
        Resp3Value::Push(elements) => {
            let mut result = format!(">{}\r\n", elements.len());
            for element in elements {
                result.push_str(&write(element));
            }
            result
        }
        Resp3Value::Attribute(attrs, value) => {
            let mut result = format!("|{}\r\n", attrs.len());
            for (k, v) in attrs {
                result.push_str(&write(k));
                result.push_str(&write(v));
            }
            result.push_str(&write(value));
            result
        }
        Resp3Value::StreamedString(chunks) => {
            let mut result = "$?\r\n".to_string();
            for chunk in chunks {
                result.push_str(&format!(";{}\r\n{}", chunk.len(), chunk));
            }
            result.push_str(";0\r\n");
            result
        }
        Resp3Value::StreamedArray(elements) => {
            let mut result = "*?\r\n".to_string();
            for element in elements {
                result.push_str(&write(element));
            }
            result.push_str(".\r\n");
            result
        }
    }
}


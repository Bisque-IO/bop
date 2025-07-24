package bopc

import runtime "base:runtime"
import "core:fmt"
import "core:io"
import log "core:log"
import "core:os"
import "core:os/os2"
import filepath "core:path/filepath"
import "core:strings"
import "core:testing"

Parser_Error :: union {}

Pos :: struct {
    index: i64, // starts at 0
    line:  i64, // starts at 1
    col:   i64, // starts at 1
}

Token_Kind :: enum {
    None,
    New_Line,
    White_Space,
    Forward_Slash,
    Back_Slash,
    Period,
    DotDot,
    Star,
    Bang,
    Comma,
    Colon,
    ColonColon,
    Semicolon,
    Pound,
    Curly_Open,
    Curly_Close,
    At,
    Quote_Double,
    Quote_Single,
    Backtick,
    Less_Than,
    Greater_Than,
    Bracket_Open,
    Bracket_Close,
    Carat,
    And,
    Question,
    Or,
    Equals,
    Plus,
    Minus,
    Comment_Line,
    Comment_Block,
    Word,
    Digit,
    EOF,
}

Token :: struct {
    file:       string,
    token:      Token_Kind,
    begin, end: Pos,
    text:       string,
}

token_string :: proc(token: Token) -> string {
    //odinfmt: disable
    switch token.token {
    case .None: return "<NONE>"
    case .New_Line: return "<LF>"
    case .White_Space: return "<WHITESPACE>"
    case .Forward_Slash: return "/"
    case .Back_Slash: return "\\"
    case .Period: return "."
    case .DotDot: return ".."
    case .Star: return "*"
    case .Bang: return "!"
    case .Comma: return ","
    case .Colon: return ":"
    case .ColonColon: return "::"
    case .Semicolon: return ";"
    case .Pound: return "#"
    case .Curly_Open: return "{"
    case .Curly_Close: return "}"
    case .At: return "@"
    case .Quote_Double: return "\""
    case .Quote_Single: return "'"
    case .Backtick: return "`"
    case .Less_Than: return "<"
    case .Greater_Than: return ">"
    case .Bracket_Open: return "["
    case .Bracket_Close: return "]"
    case .Carat: return "^"
    case .And: return "&"
    case .Question: return "?"
    case .Or: return "||"
    case .Equals: return "="
    case .Plus: return "+"
    case .Minus: return "-"
    case .Comment_Line: return token.text
    case .Comment_Block: return token.text
    case .Word: return token.text
    case .Digit: return token.text
    case .EOF: return "<EOF>"
    }
    //odinfmt: enable
    return ""
}

EOF_Bad :: Token {
    token = .None,
}
EOF :: Token {
    token = .EOF,
}

Comment_Buf :: struct {}

Numeral :: union {
    i64,
    u64,
    i128,
    u128,
    f64,
}

Parser :: struct {
    filename:           string,
    contents:           string,
    mark:               Pos,
    line_breaks:        i64,
    mark_line_breaks:   i64,
    reader:             strings.Reader,
    pos:                Pos,
    pos_prev:           Pos,
    current_char:       rune,
    current_line_index: i64,
    token_prev:         Token,
    token:              Token,
    token_next:         Token,
    comments:           Comment_Buf,
    numeral:            Maybe(Numeral),
    io_error:           io.Error,
    status:             enum u8 {
        Open,
        Closed,
    },
}

_unread_rune :: proc(parser: ^Parser) {
    if parser.pos_prev.col == 0 {
        panic("invalid state: can only unread a max of 1 rune")
    }
    strings.reader_unread_rune(&parser.reader)
    parser.pos = parser.pos_prev
    parser.pos_prev = Pos{}
}

_read_rune :: proc(parser: ^Parser) -> Maybe(rune) {
    c, size, err := strings.reader_read_rune(&parser.reader)
    if err != .None {
        parser.io_error = err
        return nil
    }
    parser.pos_prev = parser.pos
    if c == '\n' {
        parser.pos.line += 1
        parser.pos.col = 1
    } else {
        parser.pos.col += 1
    }
    parser.pos.index = parser.reader.i
    parser.current_char = c
    return c
}

_parser_set_token :: proc(parser: ^Parser, token: Token_Kind) -> Token {
    return Token {
        token = token,
        begin = parser.mark,
        end = parser.pos,
        text = parser.contents[parser.mark.index:parser.pos.index],
    }
}

_parser_next_token :: proc(self: ^Parser) -> Token {
    ch: rune
    self.mark = self.pos
    self.mark_line_breaks = self.line_breaks

    switch c in _read_rune(self) {
    case nil:
        return Token{token = .EOF}
    case rune:
        ch = c
    }

    //odinfmt: disable
    switch ch {
    case ' ', '\t', '\r':
        for {
            switch c in _read_rune(self) {
            case nil:
                return EOF_Bad
            case rune:
                ch = c
            }

            switch ch {
            case ' ', '\t', '\r':
                continue
            }
            _unread_rune(self)
            break
        }
        return _parser_set_token(self, .White_Space)

    case '\n': return _parser_set_token(self, .New_Line)
    case '\'': return _parser_set_token(self, .Quote_Single)
    case '"':  return _parser_set_token(self, .Quote_Double)
    case '?':  return _parser_set_token(self, .Question)
    case '#':  return _parser_set_token(self, .Pound)
    case '=':  return _parser_set_token(self, .Equals)
    case '!':  return _parser_set_token(self, .Bang)
    case '[':  return _parser_set_token(self, .Bracket_Open)
    case ']':  return _parser_set_token(self, .Bracket_Close)
    case '{':  return _parser_set_token(self, .Curly_Open)
    case '}':  return _parser_set_token(self, .Curly_Close)
    case ',':  return _parser_set_token(self, .Comma)
    case '|':  return _parser_set_token(self, .Or)

    case ':':
        switch c in _read_rune(self) {
        case nil:
            return EOF_Bad
        case rune:
            if ch == ':' {
                return _parser_set_token(self, .ColonColon)
            }
        }
        _unread_rune(self)
        return _parser_set_token(self, .Colon)

    case ';': return _parser_set_token(self, .Semicolon)
    case '+': return _parser_set_token(self, .Plus)
    case '-': return _parser_set_token(self, .Minus)
    case '.': return _parser_set_token(self, .Period)
    case '@': return _parser_set_token(self, .At)
    case '&': return _parser_set_token(self, .And)
    case '0', '1', '2', '3', '4', '5', '6', '7', '8', '9':
        return _parser_set_token(self, .Digit)
    case '_', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm',
    'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M',
    'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z':
        for {
            switch c in _read_rune(self) {
            case nil:
                return EOF_Bad
            case rune:
                ch = c
            }

            switch ch {
            case '_', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
            'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm',
            'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
            'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M',
            'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z':
                continue
            }
            _unread_rune(self)
            return _parser_set_token(self, .Word)
        }
    }
    //odinfmt: enable

    return _parser_set_token(self, .None)
}

is_eof :: proc {
    is_eof_code,
    is_eof_view,
    is_eof_view_ref,
}

is_eof_code :: proc(token: Token_Kind) -> bool {
    return token == .EOF
}

is_eof_view :: proc(token: Token) -> bool {
    return token.token == .EOF
}

is_eof_view_ref :: proc(token: ^Token) -> bool {
    return token.token == .EOF
}

@private
_parser_next :: proc(parser: ^Parser) -> Token {
    parser.token_prev = parser.token

    if parser.token.token == .None {
        parser.token = _parser_next_token(parser)
    } else {
        parser.token = parser.token_next
    }

    if parser.token.token == .EOF {
        return parser.token
    }

    parser.token_next = _parser_next_token(parser)
    return parser.token
}

parse_from_filename :: proc(filename: string) {
    data, success := os.read_entire_file_from_filename(filename)
    if !success {
        return
    }

    parse_from_contents(filename, string(data))


}

parse_from_contents :: proc(filename, contents: string) {
    parser := Parser {
        filename = filename,
        contents = contents,
        mark = Pos{},
        line_breaks = 0,
        mark_line_breaks = 0,
        pos = Pos{line = 1, col = 1, index = 0},
        current_char = 0,
        current_line_index = 0,
        token_prev = EOF_Bad,
        token = EOF_Bad,
        token_next = EOF_Bad,
        comments = Comment_Buf{},
        numeral = nil,
        io_error = .None,
    }
    strings.reader_init(&parser.reader, parser.contents)

    loop: for {
        token := _parser_next(&parser)
        if token.token == .EOF || token.token == .None {
            log.debug(_pretty_print_token(token))
            break loop
        }
        log.debug(_pretty_print_token(token))
    }
}

_pretty_print_token :: proc(token: Token) -> string {
    if token.begin.line == 0 {
        return fmt.tprint(token.token)
    }
    return fmt.tprint(
        token.token,
        " ",
        token.begin.line,
        ":",
        token.begin.col,
        "-",
        token.end.line,
        ":",
        token.end.col,
        " = ",
        strings.trim_space(token.text),
        sep = "",
    )
}

find_all_bop_files :: proc() {
    infos, err := os2.read_all_directory_by_path("bop", context.allocator)
    if err != nil {
        switch e in err {
        case os2.General_Error:
            fmt.println("General_Error")
            fmt.println(e)
        case os2.Platform_Error:
            fmt.println("Platform_Error")
        case io.Error:
            fmt.println(e)
        case runtime.Allocator_Error:
            fmt.println("Allocator_Error")
        }
        return
    }
    ensure(err == nil)
    defer delete(infos)
    for info in infos {
        fmt.println(info.fullpath)
    }
}

@(test)
test_parse :: proc(t: ^testing.T) {
    fmt.println("")
    fmt.println("")

    collection := new(Collection)
    collection.packages = make(map[string]^Package)
    collection.path = "crates/bop-compiler"
    defer delete(collection.packages)
    defer free(collection)

    //    fmt.println("current dir: ", os.get_current_directory(context.temp_allocator))
    //    info, err := os.lstat("crates/bop-compiler", context.temp_allocator)
    //    info, err := os.lstat("crates/bop-compiler", context.temp_allocator)
    //    ensure(err == nil)
    //    log.debug("hi")
    //    fmt.println("current dir: ", info.fullpath)
    //    current_dir := new_clone(info.fullpath, context.allocator)
    //    defer free(current_dir)
    //    if len(info.fullpath) > 0 do delete(info.fullpath)
    //    if len(info.name) > 0 do delete(info.name)

    //    walker := os2.walker_create_path("crates/bop-compiler/src/odin")
    //    infos, err := os2.read_all_directory_by_path("/home/me/repos/bisque-io/bvm/crates/bop-compiler", context.allocator)
    //
    //    if err != nil {
    //        switch e in err {
    //        case os2.General_Error:
    //            fmt.println("General_Error")
    //        case os2.Platform_Error:
    //            fmt.println("Platform_Error")
    //        case io.Error:
    //            fmt.println(e)
    //        case runtime.Allocator_Error:
    //            fmt.println("Allocator_Error")
    //        }
    //        return
    //    }
    //    fmt.println(err)
    //    ensure(err == nil)
    //    defer delete(infos)
    //    for info in infos {
    //        fmt.println(info.fullpath)
    //    }

    filepath.walk(
        "crates/bop-compiler/src/odin",
        proc(
            info: os.File_Info,
            in_err: os.Error,
            user_data: rawptr,
        ) -> (
            err: os.Error,
            skip_dir: bool,
        ) {
            if filepath.ext(info.fullpath) == ".bop" {
                collection := cast(^Collection)user_data
                log.debug(info.fullpath)
                log.debug(filepath.dir(info.fullpath, context.temp_allocator))
                log.debug(filepath.stem(info.fullpath))
            }
            return nil, false
        },
        rawptr(collection),
    )

    filename := "crates/bop-compiler/src/odin/schema.bop"
    data, ok := os.read_entire_file_from_filename(filename)
    ensure(ok, "failed to read file")
    defer delete(data)

    parse_from_contents(filename, string(data))

    log.debug("\n", string(data), "\n", sep = "")
//    fmt.println(string(data))
}

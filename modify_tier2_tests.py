from pathlib import Path

path = Path("rust/bop-rs/src/aof2/store/tier2.rs")
text = path.read_text()
old = "    use std::collections::HashMap as StdHashMap;\n    use std::time::Duration;\n"
new = "    use std::collections::HashMap as StdHashMap;\n    use std::sync::Arc;\n    use std::time::Duration;\n"
if old not in text:
    raise SystemExit('tests import block not found')
path.write_text(text.replace(old, new))

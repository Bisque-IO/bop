# Python Test Wrapper

This snippet runs an individual Cargo test via Python. It mirrors the command we used while investigating the recovery tests.

```python
import subprocess
import sys

result = subprocess.run(
    ["cargo", "test", "--lib", "aof2::tests::recovery_reopens_existing_tail_segment"],
    check=False,
)
sys.exit(result.returncode)
```

Run from `rust/bop-rs/`:

```bash
python -c "import subprocess, sys; sys.exit(subprocess.run(['cargo','test','--lib','aof2::tests::recovery_reopens_existing_tail_segment'], check=False).returncode)"
```

Update the test name in the list argument to exercise a different test case.

import os
import platform
import subprocess
import sys
import shutil
from dataclasses import dataclass
from typing import List

# Platform detection
IS_WINDOWS = platform.system() == "Windows"
IS_LINUX = platform.system() == "Linux"
IS_MAC = platform.system() == "Darwin"

arch = platform.machine().lower()

IS_AMD64 = False
IS_ARM64 = False
IS_RISCV64 = False
LIB_DIR = ""
SO_SUFFIX = ""

if IS_LINUX:
    SO_SUFFIX = ".so"
elif IS_MAC:
    SO_SUFFIX = ".dylib"
elif IS_WINDOWS:
    SO_SUFFIX = ".dll"

# Normalize to common arch names
if arch in ("x86_64", "amd64"):
    IS_AMD64 = True
    if IS_LINUX:
        LIB_DIR = "odin/libbop/linux/amd64"
    elif IS_MAC:
        LIB_DIR = "odin/libbop/macos/amd64"
    elif IS_WINDOWS:
        LIB_DIR = "odin/libbop/windows/amd64"
elif arch in ("aarch64", "arm64"):
    IS_ARM64 = True
    if IS_LINUX:
        LIB_DIR = "odin/libbop/linux/arm64"
    elif IS_MAC:
        LIB_DIR = "odin/libbop/macos/arm64"
    elif IS_WINDOWS:
        LIB_DIR = "odin/libbop/windows/arm64"
elif arch in ("riscv64",):
    IS_RISCV64 = True
    if IS_LINUX:
        LIB_DIR = "odin/libbop/linux/riscv64"
else:
    print(f"Unknown architecture: {arch}")
    sys.exit(-1)

@dataclass
class Test:
    path: str
    name: str

def boxify(text: str, padding: int = 1) -> str:
    lines = text.splitlines()
    max_len = max(len(line) for line in lines)
    pad = " " * padding
    top = "┌" + "─" * (max_len + padding * 2) + "┐"
    bottom = "└" + "─" * (max_len + padding * 2) + "┘"
    middle = [f"│{pad}{line.ljust(max_len)}{pad}│" for line in lines]
    return "\n".join([top] + middle + [bottom])

def print_odin_version():
    try:
        subprocess.run(["odin", "version"], check=True)
    except subprocess.CalledProcessError as e:
        print(f"❌  FAILED with exit code {e.returncode}\n")
        sys.exit(e.returncode)

def print_odin_report():
    try:
        result = subprocess.run(["odin", "report"], check=True, capture_output=True, text=True)
        output = result.stdout.strip()
        index = output.index("Odin:")
        if index > -1:
            output = output[index:]
        trimmed = "\n".join(line.lstrip() for line in output.splitlines())
        print(boxify(trimmed))
        print("")
    except subprocess.CalledProcessError as e:
        print(f"❌  FAILED with exit code {e.returncode}\n")
        sys.exit(e.returncode)

def format_size(bytes, suffix="B"):
    units = ["", "K", "M", "G", "T", "P"]
    for unit in units:
        if bytes < 1024.0:
            return f"{bytes:3.1f} {unit}{suffix}"
        bytes /= 1024.0
    return f"{bytes:.1f} P{suffix}"

def run_odin_tests(
    path: str,
    name: str = "",
    debug: bool = False,
    keep_build: bool = False,
    build_only: bool = False
):
    cmd = ["odin", "test"]
    cmd += [path]

    # if path == "./" or path == "" or path == ".":
    #     cmd += ["-all-packages"]

    cmd += ["-define:ODIN_TEST_THREADS=1"]
    cmd += ["-define:ODIN_TEST_FANCY=false"]
    cmd += ["-define:ODIN_TEST_TRACK_MEMORY=true"]
    cmd += ["-define:ODIN_TEST_ALWAYS_REPORT_MEMORY=false"]
    cmd += ["-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true"]
    cmd += ["-define:ODIN_TEST_RANDOM_SEED=151"]
    # cmd += ["-define:ODIN_TEST_JSON_REPORT=test-results.json"]
    cmd += ["-show-timings"]

    if keep_build:
        cmd += ["-keep-executable"]

    test_bin = os.path.join(path, "test-bin")
    os.makedirs(test_bin, 777, True)

    exe_name = ""
    if len(name) > 0:
        exe_name = name.replace(".", "_")
        exe_name = exe_name.replace(",", "_")
    else:
        exe_name = os.path.basename(os.path.normpath(path)) + "_test"

    if IS_WINDOWS:
        exe_name += ".exe"

    exe_path = os.path.join(test_bin, exe_name)
    cmd += [f"-out:{exe_path}"]

    if len(name) > 0:
        cmd += [f"-define:ODIN_TEST_NAMES='{name}'"]
    else:
        print(f"testing all tests in package: {path}")

    # if build_only:
    #     cmd += ["-build-only"]

    if debug:
        cmd += ["-debug"]
        cmd += ["-define:BOP_DEBUG=1"]

    if IS_WINDOWS:
        cmd += ["-linker:default"]
        linker_flags = "/ignore:4099 /NODEFAULTLIB:libcmt"
        cmd += [f"-extra-linker-flags:{linker_flags}"]
    else:
        cmd += ["-linker:lld"]

    print(f"\n🔧 running test(s):\n\n\t{' '.join(cmd)}\n")
    print_odin_report()
    try:
        subprocess.run(cmd, check=True)
        print(f"✅  SUCCESS\n")
    except subprocess.CalledProcessError as e:
        print(f"\n❌  FAILED with exit code {e.returncode}\n")
        sys.exit(e.returncode)


USAGE = """
Usage:
        ./forge command [arguments]
Commands:
        build             Compiles directory of .odin files, as an executable.
                          One must contain the program's entry point, all must be in the same package.
        run               Same as 'build', but also then runs the newly compiled executable.
        check             Parses and type checks a directory of .odin files.
        test              Builds and runs procedures with the attribute @(test) in the initial package.
        doc               Generates documentation from a directory of .odin files.
        version           Prints version.
        root              Prints the root path where Odin looks for the builtin collections.

"""

ODIN_TEST_USAGE = """
./forge test {dir} [optional test name] [optional test name] ...
"""

def test(args: List[str]):
    if not args or len(args) == 0:
        print(ODIN_TEST_USAGE)
        sys.exit(-1)

    debug = False
    keep_build = False
    build_only = False
    split = False

    additional_options = []

    path = args[0]
    if path == "-h" or path == "-help":
        print(ODIN_TEST_USAGE)
        sys.exit(-1)

    if not os.path.exists(path) or not os.path.isdir(path):
        print(f"\n❌  '{path}' is not a directory\n")
        sys.exit(-1)

    args = args[1:]
    test_names = []

    for arg in args:
        if arg == "-h" or arg == "-help":
            print(ODIN_TEST_USAGE)
            sys.exit(-1)
        if arg == "-debug":
            debug = True
            continue
        if arg == "-keep-build":
            keep_build = True
            continue
        if arg == "-keep_build-only":
            build_only = True
            continue
        if arg == "-split":
            split = True
            continue

        if arg.startswith("-"):
            additional_options += [arg]
            continue

        test_names += [arg]

    if len(test_names) > 0:
        if split:
            for test_name in test_names:
                run_odin_tests(path, test_name, debug, keep_build, build_only)
        else:
            run_odin_tests(path, ",".join(test_names), debug, keep_build, build_only)
    else:
        run_odin_tests(path, "", debug, keep_build, build_only)



ODIN_BUILD_USAGE = """
forge is a tool for managing bop source code.

Usage:
        ./forge build [arguments]

        build   Compiles directory of .odin files as an executable.
                One must contain the program's entry point, all must be in the same package.
                Examples:
                        ./forge build <dir>                 Builds package in <dir>.
                        ./forge build filename.odin         Builds single-file package, must contain entry point.

        Flags
        
        -kind:<mode>
                Sets the build mode.
                Available options:
                        -kind:exe         Builds as an executable.
                        -kind:test        Builds as an executable that executes tests.
                        -kind:dll         Builds as a dynamically linked library.
                        -kind:shared      Builds as a dynamically linked library.
                        -kind:dynamic     Builds as a dynamically linked library.
                        -kind:lib         Builds as a statically linked library.
                        -kind:static      Builds as a statically linked library.
                        -kind:obj         Builds as an object file.
                        -kind:object      Builds as an object file.
                        -kind:assembly    Builds as an assembly file.
                        -kind:assembler   Builds as an assembly file.
                        -kind:asm         Builds as an assembly file.
                        -kind:llvm-ir     Builds as an LLVM IR file.
                        -kind:llvm        Builds as an LLVM IR file.

        -show-defineables
                Shows an overview of all the #config/#defined usages in the project.

        -show-system-calls
                Prints the whole command and arguments for calls to external tools like linker and assembler.
                
        -microarch:<string>
                Specifies the specific micro-architecture for the build in a string.
                Examples:
                        -microarch:sandybridge
                        -microarch:native
                        -microarch:"?" for a list
                        
        -strict-style
                This enforces parts of same style as the Odin compiler, prefer '-vet-style -vet-semicolon' if you do not want to match it exactly.

                Errs on unneeded tokens, such as unneeded semicolons.
                Errs on missing trailing commas followed by a newline.
                Errs on deprecated syntax.
                Errs when the attached-brace style in not adhered to (also known as 1TBS).
                Errs when 'case' labels are not in the same column as the associated 'switch' token.
                
        -vet
                Does extra checks on the code.
                Extra checks include:
                        -vet-unused
                        -vet-unused-variables
                        -vet-unused-imports
                        -vet-shadowing
                        -vet-using-stmt

        -vet-cast
                Errs on casting a value to its own type or using `transmute` rather than `cast`.

        -vet-packages:<comma-separated-strings>
                Sets which packages by name will be vetted.
                Files with specific +vet tags will not be ignored if they are not in the packages set.

        -vet-semicolon
                Errs on unneeded semicolons.

        -vet-shadowing
                Checks for variable shadowing within procedures.

        -vet-style
                Errs on missing trailing commas followed by a newline.
                Errs on deprecated syntax.
                Does not err on unneeded tokens (unlike -strict-style).

        -vet-tabs
                Errs when the use of tabs has not been used for indentation.

        -vet-unused
                Checks for unused declarations (variables and imports).

        -vet-unused-imports
                Checks for unused import declarations.

        -vet-unused-procedures
                Checks for unused procedures.
                Must be used with -vet-packages or specified on a per file with +vet tags.

        -vet-unused-variables
                Checks for unused variable declarations.

        -vet-using-param
                Checks for the use of 'using' on procedure parameters.
                'using' is considered bad practice outside of immediate refactoring.

        -vet-using-stmt
                Checks for the use of 'using' as a statement.
                'using' is considered bad practice outside of immediate refactoring.

        -warnings-as-errors
                Treats warning messages as error messages.
"""

def build(args: List[str]):
    if len(args) == 0:
        print(USAGE)
        sys.exit(-1)

    path = args[0]
    dir = path
    name = path

    is_file = path.endswith(".odin")
    if is_file:
        dir = os.path.dirname(path)
        name = os.path.basename(path)
    else:
        name = os.path.basename(os.path.normpath(path))

    bin_dir = os.path.join(dir, "bin")
    os.makedirs(bin_dir, 777, True)

    debug = True
    shared = False
    release = False
    openssl = False
    hide_timings = False
    sanitize_address = False
    sanitize_memory = False
    sanitize_thread = False
    static_exe = False
    run = False
    opt = "none"
    kind = "exe"
    show_system_calls = False
    show_defineables = False
    strict_style = False
    vet = False
    vet_cast = False
    vet_semicolon = False
    vet_style = False
    vet_tabs = False
    vet_unused = False
    vet_unused_imports = False
    vet_unused_procedures = False
    vet_unused_variables = False
    vet_using_param = False
    vet_using_stmt = False
    warnings_as_errors = False
    microarch = ""
    no_cmt = False

    for arg in args:
        if arg == "-h" or arg == "-help":
            print(ODIN_BUILD_USAGE)
            sys.exit(-1)
        if arg == "-debug":
            debug = True
            continue
        if arg == "-release":
            release = True
            opt = "aggressive"
            continue
        if arg == "-openssl":
            openssl = True
            continue
        if arg == "-shared":
            shared = True
            continue
        if arg == "-hide-timings":
            hide_timings = True
            continue
        if arg == "-run":
            run = True
            continue
        if arg == "-static-exe":
            static_exe = True
            continue
        if arg.startswith("-kind:"):
            kind = arg.removeprefix("-kind:")
            continue
        if arg == "-sanitize-address":
            sanitize_address = True
            continue
        if arg == "-sanitize-memory":
            sanitize_memory = True
            continue
        if arg == "-sanitize-thread":
            sanitize_thread = True
            continue
        if arg == "-show-defineables":
            show_defineables = True
            continue
        if arg == "-show-system-calls":
            show_system_calls = True
            continue
        if arg == "-vet":
            vet = True
            continue
        if arg == "-vet-cast":
            vet_cast = True
            continue
        if arg == "-vet-semicolon":
            vet_semicolon = True
            continue
        if arg == "-vet-style":
            vet_style = True
            continue
        if arg == "-vet-tabs":
            vet_tabs = True
            continue
        if arg == "-vet-unused":
            vet_unused = True
            continue
        if arg == "-vet-unused-imports":
            vet_unused_imports = True
            continue
        if arg == "-vet-unused-procedures":
            vet_unused_procedures = True
            continue
        if arg == "-vet-unused-variables":
            vet_unused_variables = True
            continue
        if arg == "-vet-using-param":
            vet_using_param = True
            continue
        if arg == "-vet-using-stmt":
            vet_using_stmt = True
            continue
        if arg == "-warnings-as-errors":
            warnings_as_errors = True
            continue

        if arg.startswith("-microarch:"):
            microarch = arg.removeprefix("-microarch:")

    cmd = ["odin", "build", path]

    if is_file:
        cmd += ["-file"]

    exe_path = os.path.join(bin_dir, name)

    extension = ""
    if kind == "exe":
        extension = ".exe" if IS_WINDOWS else ""
    elif kind == "dll" or kind == "shared":
        if IS_MAC:
            extension = ".dylib"
        elif IS_WINDOWS:
            extension = ".dll"
        else:
            extension = ".so"
    elif kind == "static" or kind == "lib":
        if IS_MAC:
            extension = ".a"
        elif IS_WINDOWS:
            extension = ".lib"
        else:
            extension = ".a"
    elif kind == "assembly" or kind == "assembler" or kind == "asm":
        extension = ".S"
    elif kind == "obj" or kind == "object":
        extension = ".o"
    elif kind == "llvm-ir" or kind == "llvm":
        extension = ".ir"

    cmd += [f"-build-mode:{kind}"]

    if debug:
        cmd += ["-debug"]

    if sanitize_address:
        cmd += ["-sanitize:address"]
    if sanitize_memory:
        cmd += ["-sanitize:memory"]
    if sanitize_thread:
        cmd += ["-sanitize:thread"]

    if show_defineables:
        cmd += ["-show-defineables"]
    if show_system_calls:
        cmd += ["-show-system-calls"]

    if strict_style:
        cmd += ["-strict-style"]
    if vet:
        cmd += ["-vet"]
    if vet_cast:
        cmd += ["-vet-cast"]
    if vet_semicolon:
        cmd += ["-vet-semicolon"]
    if vet_style:
        cmd += ["-vet-style"]
    if vet_tabs:
        cmd += ["-vet-tabs"]
    if vet_unused:
        cmd += ["-vet-unused"]
    if vet_unused_imports:
        cmd += ["-vet-unused-imports"]
    if vet_unused_procedures:
        cmd += ["-vet-unused-procedures"]
    if vet_unused_variables:
        cmd += ["-vet-unused-variables"]
    if vet_using_param:
        cmd += ["-vet-using-param"]
    if vet_using_stmt:
        cmd += ["-vet-using-stmt"]
    if warnings_as_errors:
        cmd += ["-warnings-as-errors"]

    exe_path += extension

    if IS_WINDOWS:
        cmd += ["-o:" + opt]
        cmd += [f"-out:{exe_path}"]
        # cmd += ["-define:BOP_SHARED=0"]
        cmd += ["-linker:default"]
        cmd += ["-show-timings"]
        # linker_flags = "/ignore:4099 /NODEFAULTLIB:libcmt /MAP"
        linker_flags = "/ignore:4099"
        cmd += [f"-extra-linker-flags:{linker_flags}"]
    else:
        if shared:
            cmd += ["-define:BOP_SHARED=1"]
            if openssl:
                shutil.copy(
                    os.path.join(LIB_DIR, "libbop-openssl") + SO_SUFFIX,
                    os.path.join(bin_dir, "libbop-openssl") + SO_SUFFIX
                )
            else:
                shutil.copy(
                    os.path.join(LIB_DIR, "libbop") + SO_SUFFIX,
                    os.path.join(bin_dir, "libbop") + SO_SUFFIX
                )

        cmd += ["-o:" + opt]
        cmd += [f"-out:{exe_path}"]
        # cmd += ["-define:BOP_DEBUG=0"]
        # cmd += ["-define:BOP_SHARED=0"]
        if openssl:
            cmd += ["-define:BOP_OPENSSL=1"]
        cmd += ["-linker:lld"]
        cmd += ["-show-timings"]
        linker_flags = "-rdynamic"
        if static_exe:
            linker_flags += " -static"
        cmd += [f"-extra-linker-flags:\"{linker_flags}\""]

    if len(microarch) > 0:
        cmd += [f"-microarch:{microarch}"]

    print(f"\n🔧 building '{path}':\n\n\t{' '.join(cmd)}\n")
    print_odin_report()
    try:
        subprocess.run(cmd, check=True)


        size = os.path.getsize(exe_path)
        print(f"\n✅  {exe_path} => {format_size(size)}\n")

        if run:
            print("running...")
            try:
                subprocess.run(exe_path, check=True)
            except subprocess.CalledProcessError as e:
                print(f"exited with exit code {e.returncode}\n")
            except KeyboardInterrupt:
                print("")
    except subprocess.CalledProcessError as e:
        print(f"\n❌  '{path}' failed with exit code {e.returncode}\n")
        sys.exit(e.returncode)

def release(args: List[str]):
    """

    :return:
    """

def main():
    if len(sys.argv) < 2:
        print(USAGE)
        sys.exit(-1)

    cmd = sys.argv[1]
    args = sys.argv[2:]

    if len(cmd) == 0:
        print(USAGE)
        sys.exit(-1)

    if cmd == "test":
        test(args)
    elif cmd == "build" or cmd == "b":
        build(args)
    elif cmd == "release":
        release(args)
    elif cmd == "version":
        print_odin_report()
    else:
        print(USAGE)
        sys.exit(-1)


if __name__ == "__main__":
    main()

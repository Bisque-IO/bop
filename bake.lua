---@diagnostic disable: undefined-field
-- conan remote update conancenter --url https://center2.conan.io

local global = import("core.base.global")
--hash = import("core.base.hash")
local bytes = import("core.base.bytes")

local host = os.host()
local HOST = os.host()
local ARCH = os.arch()

local logo1 = [[
  _     _
 | |   (_)
 | |__  _ ___  __ _ _   _  ___
 | '_ \| / __|/ _` | | | |/ _ \
 | |_) | \__ \ (_| | |_| |  __/
 |_.__/|_|___/\__, |\__,_|\___|
                 |_| v0.1.0
]]

local logo = [[
 _
| |
| |__   ___  ____
|  _ \ / _ \|  _ \
| |_) ) |_| | |_| |
|____/ \___/|  __/
  v0.1.0    |_|
]]

local help = [[
bake is a tool for managing bop source code development.

Usage:

    ./bake <command> [arguments]

The commands are:

    configure (c)   configures project and cmake
        --debug     configures project as debug   (same as './bake configure --debug')
        --release   configures project as release (same as './bake configure --release')

    build     (b)   build source code
    clean           remove build files and cached files
    env             print environment information
    setup           setup development environment
    version         print version

Use "./bake help <command>" for more information about a command.
]]

function main(...)
--     print(logo)
    local vararg = {...}
    if vararg[1] == "ide" then
        print("generating compile_commands.json...")
        os.exec("xmake project -k compile_commands")
        os.cp("compile_commands.json", ".vscode/compile_commands.json")
        print("generating CMakeLists.txt...")
        os.exec("xmake project -k cmakelists")

    elseif vararg[1] == "configure" or vararg[1] == "c" or vararg[1] == "--debug" or vararg[1] == "--release" then
        os.rm("build")
        os.rm(".xmake")

        local debug = true
        local release = vararg[1] == "--release"
        local llvm = false
        local verbose = false
        local install_packages = true

        for i=2, #vararg do
            if vararg[i] == "release" then
                release = true
            elseif vararg[i] == "--llvm" then
                llvm = true
            elseif vararg[i] == "-v" then
                verbose = true
            end
        end

        local toolchain = ""
        if llvm then
            local llvm_config = os.iorun("sh llvm-config.sh --prefix")
            if string.len(llvm_config) == 0 then
                print("llvm-config could not be found!")
                return
            end
            print("llvm-config: " .. llvm_config)
            local llvm_path = os.iorun(llvm_config .. " --prefix")
            print("llvm sdk:    " .. llvm_path)
            local llvm_version = os.iorun(llvm_config .. " --version")
            print("llvm version: " .. llvm_version)
            local index_of_dot = string.find(llvm_version, "[.]")
            local llvm_major = string.sub(llvm_version, 1, index_of_dot-1)
            print("llvm major version: " .. llvm_major)

            toolchain = " --toolchain=clang-" .. llvm_major
            --toolchain = " --toolchain=llvm --sdk=" .. llvm_path
        end

        local cmd = "xmake f "
        if verbose then
            cmd = cmd .. "-v "
        end
        if release then
            cmd = cmd .. "-m release"
        else
            cmd = cmd .. "-m debug"
        end

        if install_packages then
            cmd = cmd .. " -y"
        end

        cmd = cmd .. toolchain

        print("configure command: " .. cmd)
        os.exec(cmd)

        print("generating compile_commands.json...")
        os.exec("xmake project -k compile_commands")
        os.cp("compile_commands.json", ".vscode/compile_commands.json")

        print("generating CMakeLists.txt...")
        os.exec("xmake project -k cmakelists")

    elseif vararg[1] == "clean" then
        os.rm("build")
        os.rm(".xmake")

    elseif vararg[1] == "run" then
        local args = ""
        for i=2, #vararg do
            if i > 2 then
                args = args .. " "
            end
            args = args .. vararg[i]
        end
        os.exec("xmake b " .. vararg[2])
        print("running command:")
        print()
        print("\t" .. args)
        print()
        os.exec("xmake r " .. args)

    elseif vararg[1] == "build" then
        os.exec("xmake")

    elseif vararg[1] == "env" then
        print("OS:           " .. os.host())
        print("CPU arch:     " .. os.arch())
        local cpu_info = os.cpuinfo()
        print("CPU vendor:   " .. cpu_info.vendor)
        print("CPU model:    " .. cpu_info.model_name)
        print("CPU march:    " .. cpu_info.march)
        print("CPU cores:    " .. cpu_info.ncpu)
        print("CPU features: " .. cpu_info.features)
        print()
        print("Working Dir:  " .. os.workingdir())
        print("Project Dir:  " .. os.projectdir())
        print("Program Dir:  " .. os.programdir())
--         print("Program Dir:  " .. os.builddir())
--         print(os)
        local mem_info = os.meminfo()
        print("Page Size:    " .. mem_info.pagesize)
        print("Total Size:   " .. mem_info.totalsize .. "mb")
        print("Avail Size:   " .. mem_info.availsize .. "mb")
        print("Used:         " .. mem_info.usagerate)
        print()
        print("toolchains:")
        os.exec("xmake show -l toolchains")

    elseif vararg[1] == "ls" then
        local debug_dir = "build/" .. os.host() .. "/" .. os.arch() .. "/debug"
        local release_dir = "build/" .. os.host() .. "/" .. os.arch() .. "/release"

        if os.isdir(debug_dir) then
            print("debug: " .. debug_dir)
            if HOST == "windows" then
                os.exec("powershell -Command \"dir " .. debug_dir .. "\"")
            else
                os.exec("ls -l " .. debug_dir)
            end
            print()
        end
        if os.isdir(release_dir) then
            print("release: " .. release_dir)

            if HOST == "windows" then
                os.exec("powershell -Command \"dir " .. release_dir .. "\"")
            else
                os.exec("ls -l " .. release_dir)
            end
        end

        local ldd_app = ""
        local so_prefix = ""
        local so_suffix = ""
        local archive_suffix = ".a"
        local exe_suffix = ""
        if host == "linux" then
            ldd_app = "ldd"
            so_prefix = "lib"
            so_suffix = ".so"
        elseif host == "macosx" then
            ldd_app = "otool -L"
            so_prefix = "lib"
            so_suffix = ".dylib"
        elseif HOST == "windows" then
            ldd_app = "dumpbin /dependents"
            so_suffix = ".dll"
            archive_suffix = ".lib"
            exe_suffix = ".exe"
        end
        
        local ldd = function(binary_name)
            if ldd_app == "" then return end
            local debug_path = debug_dir .. "/" .. binary_name
            local release_path = release_dir .. "/" .. binary_name

            if os.exists(debug_path) then
                print()
                print(binary_name .. " (debug)")
                os.exec(ldd_app .. " " .. debug_path)
            end
            if os.exists(release_path) then
                print()
                print(binary_name .. " (release)")
                os.exec(ldd_app .. " " .. release_path)
            end
        end

        ldd("test-ev-loop" .. exe_suffix)
        ldd("test-http-parser" .. exe_suffix)
        ldd(so_prefix .. "bisque" .. so_suffix)
        ldd(so_prefix .. "usockets-shared" .. so_suffix)
        ldd("test-curl" .. exe_suffix)
        ldd("bisqued" .. exe_suffix)
    else
        print(logo)
        print(help)
        -- print(os)
    end
    --print(logo)
    --print(help)
    --print(vararg[1])
    --print(vararg[2])
    --print(os.iorunv("xmake", {"project", "-k", "cmakelists"}))
    --print("hi")
    --print(os)
    --print(os.arch())
    --print(os.cpuinfo())
    --print(path)
    --print("global", global)
    --print("hash", hash)
    --print(hash.uuid4())
    --print(hash.sha1(bytes.new(hash.uuid4())))
    --print(hash.sha1(bytes(hash.uuid4())))
    --print("bytes", bytes("hi"):size())
    --print("meminfo", os.meminfo())
    --print(os.files("**/*.lua"))
    ----os.exec("xmake", {"version"}, {})
    ----os.exec("whereis xmake")
    ----os.execv("whereis", {"cp"})
    ----os.execv("xmake", {"show"})
    --print(os.iorunv("xmake", {"show"}))
    --print(os.iorunv("whereis", {"xmake"}))
    --print(os)
--     print(os.shell())
--     print(os.host())
end
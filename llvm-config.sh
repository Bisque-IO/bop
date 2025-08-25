#!/usr/bin/env sh

: ${LLVM_CONFIG=}

# Brew advises people not to add llvm to their $PATH, so try and use brew to find it.
if [ -z "$LLVM_CONFIG" ] &&  [ -n "$(command -v brew)" ]; then
    if   [ -n "$(command -v $(brew --prefix llvm@20)/bin/llvm-config)" ]; then LLVM_CONFIG="$(brew --prefix llvm@20)/bin/llvm-config"
    elif   [ -n "$(command -v $(brew --prefix llvm@19)/bin/llvm-config)" ]; then LLVM_CONFIG="$(brew --prefix llvm@19)/bin/llvm-config"
    elif [ -n "$(command -v $(brew --prefix llvm@18)/bin/llvm-config)" ]; then LLVM_CONFIG="$(brew --prefix llvm@18)/bin/llvm-config"
    elif [ -n "$(command -v $(brew --prefix llvm@17)/bin/llvm-config)" ]; then LLVM_CONFIG="$(brew --prefix llvm@17)/bin/llvm-config"
    elif [ -n "$(command -v $(brew --prefix llvm@14)/bin/llvm-config)" ]; then LLVM_CONFIG="$(brew --prefix llvm@14)/bin/llvm-config"
    fi
fi

if [ -z "$LLVM_CONFIG" ]; then
	# darwin, linux, openbsd
	if   [ -n "$(command -v llvm-config-20)" ]; then LLVM_CONFIG="llvm-config-20"
	elif   [ -n "$(command -v llvm-config-19)" ]; then LLVM_CONFIG="llvm-config-19"
	elif   [ -n "$(command -v llvm-config-18)" ]; then LLVM_CONFIG="llvm-config-18"
	elif [ -n "$(command -v llvm-config-17)" ]; then LLVM_CONFIG="llvm-config-17"
	elif [ -n "$(command -v llvm-config-14)" ]; then LLVM_CONFIG="llvm-config-14"
	elif [ -n "$(command -v llvm-config-13)" ]; then LLVM_CONFIG="llvm-config-13"
	elif [ -n "$(command -v llvm-config-12)" ]; then LLVM_CONFIG="llvm-config-12"
	elif [ -n "$(command -v llvm-config-11)" ]; then LLVM_CONFIG="llvm-config-11"
	# freebsd
	elif [ -n "$(command -v llvm-config20)" ]; then  LLVM_CONFIG="llvm-config20"
	elif [ -n "$(command -v llvm-config19)" ]; then  LLVM_CONFIG="llvm-config19"
	elif [ -n "$(command -v llvm-config18)" ]; then  LLVM_CONFIG="llvm-config18"
	elif [ -n "$(command -v llvm-config17)" ]; then  LLVM_CONFIG="llvm-config17"
	elif [ -n "$(command -v llvm-config14)" ]; then  LLVM_CONFIG="llvm-config14"
	elif [ -n "$(command -v llvm-config13)" ]; then  LLVM_CONFIG="llvm-config13"
	elif [ -n "$(command -v llvm-config12)" ]; then  LLVM_CONFIG="llvm-config12"
	elif [ -n "$(command -v llvm-config11)" ]; then  LLVM_CONFIG="llvm-config11"
	# fallback
	elif [ -n "$(command -v llvm-config)" ]; then LLVM_CONFIG="llvm-config"
	else
		error "No llvm-config command found. Set LLVM_CONFIG to proceed."
	fi
fi

echo -n "$LLVM_CONFIG"
exit 0

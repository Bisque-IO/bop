package bop.c.hash;

import static bop.c.Loader.LINKER;
import static bop.c.Loader.LOOKUP;

import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;

public interface CFunctions {
  MethodHandle BOP_RAPIDHASH = LINKER.downcallHandle(
      LOOKUP.find("bop_rapidhash").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t hash
          ValueLayout.JAVA_LONG, // uint8_t* data
          ValueLayout.JAVA_LONG // size_t len
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_RAPIDHASH_SEGMENT = LINKER.downcallHandle(
      LOOKUP.find("bop_rapidhash_segment").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t hash
          ValueLayout.ADDRESS, // uint8_t* data
          ValueLayout.JAVA_LONG, // size_t offset
          ValueLayout.JAVA_LONG // size_t len
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3 = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t hash
          ValueLayout.JAVA_LONG, // uint8_t* data
          ValueLayout.JAVA_LONG // size_t len
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_SEGMENT = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_segment").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t hash
          ValueLayout.ADDRESS, // uint8_t* data
          ValueLayout.JAVA_LONG, // size_t offset
          ValueLayout.JAVA_LONG // size_t len
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_ALLOC = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_alloc").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG // uint64_t address
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_DEALLOC = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_dealloc").orElseThrow(),
      FunctionDescriptor.ofVoid(
          ValueLayout.JAVA_LONG // void* state
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_UPDATE = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_update").orElseThrow(),
      FunctionDescriptor.ofVoid(
          ValueLayout.JAVA_LONG, // uint64_t hash
          ValueLayout.JAVA_LONG, // void* state
          ValueLayout.JAVA_LONG, // uint8_t* data
          ValueLayout.JAVA_LONG // size_t len
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_UPDATE_SEGMENT = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_update_segment").orElseThrow(),
      FunctionDescriptor.ofVoid(
          ValueLayout.JAVA_LONG, // void* state
          ValueLayout.ADDRESS, // uint8_t* data
          ValueLayout.JAVA_LONG, // size_t offset
          ValueLayout.JAVA_LONG // size_t len
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_UPDATE_FINAL = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_update_final").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t hash
          ValueLayout.JAVA_LONG, // void* state
          ValueLayout.JAVA_LONG, // uint8_t* data
          ValueLayout.JAVA_LONG // size_t len
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_UPDATE_FINAL_SEGMENT = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_update_final_segment").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t hash
          ValueLayout.JAVA_LONG, // void* state
          ValueLayout.ADDRESS, // uint8_t* data
          ValueLayout.JAVA_LONG, // size_t offset
          ValueLayout.JAVA_LONG // size_t len
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_UPDATE_FINAL_RESET = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_update_final_reset").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t hash
          ValueLayout.JAVA_LONG, // void* state
          ValueLayout.JAVA_LONG, // uint8_t* data
          ValueLayout.JAVA_LONG // size_t len
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_UPDATE_FINAL_RESET_SEGMENT = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_update_final_reset_segment").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t hash
          ValueLayout.JAVA_LONG, // void* state
          ValueLayout.ADDRESS, // uint8_t* data
          ValueLayout.JAVA_LONG, // size_t offset
          ValueLayout.JAVA_LONG // size_t len
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_DIGEST = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_digest").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t hash
          ValueLayout.JAVA_LONG // void* state
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_DIGEST_RESET = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_digest_reset").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t hash
          ValueLayout.JAVA_LONG // void* state
          ),
      Linker.Option.critical(true));

  MethodHandle BOP_XXH3_RESET = LINKER.downcallHandle(
      LOOKUP.find("bop_xxh3_reset").orElseThrow(),
      FunctionDescriptor.ofVoid(
          ValueLayout.JAVA_LONG // void* state
          ),
      Linker.Option.critical(true));
}

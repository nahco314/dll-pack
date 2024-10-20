# dll-pack

A toolchain that handles native-dlls and wasm-dlls with a single interface and distributes them with their dependencies via a single URL.

This toolchain has been built for the tool called “foro”, and documents, licence etc. will be prepared after the release of foro.

## platforms

- linux: fully supported
- macos: You can use WASM module, native is WIP
  - loading logic may work with the same one as linux.
  - in building tools (dll-pack-builder and GitHub Actions), we can't use `ldd`.
    - so I think we should use `otool -L`, but there is no Python-wrapping (sad)
- Windows: You can use WASM module, native is WIP
  - loading logic may work with some modification.
  - building tools need to be created.

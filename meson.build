project('context', 'rust',
  version : '0.70.0',
  license : 'unlicense',
  default_options : [
    'optimization=3',
    'strip=true',
    'lto=full',
  ],
)

gen_cross = find_program('gen-cross.sh')
run_command(gen_cross, check : true)

build_dir = meson.project_build_root() / 'cargo'
src_dir = meson.project_source_root()

custom_target('context',
  build_by_default : true,
  build_always_stale : true,
  install : true,
  install_dir : get_option('prefix') / 'bin',
  command : ['sh', '-c',
    'CARGO_TARGET_DIR={0} cargo build --release --locked --manifest-path {1}/Cargo.toml && cp {0}/release/context @OUTPUT@'
    .format(build_dir, src_dir),
  ],
  output : 'context',
)

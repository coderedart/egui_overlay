### Glfw window backend for egui

### Emscripten 
you will need to add these link flags to your build process.
I just put all of them here, but choose what you need.
```toml
# inside .cargo/config.toml
[target.wasm32-unknown-emscripten]
rustflags = [
    "-C",
    "link-arg=-s",
    "-C",
    "link-arg=USE_GLFW=3", # for glfw support. 
    # "-C",
    # "link-arg=-s",
    # "-C",
    # "link-arg=FULL_ES2",# for opengl es 2 emulation
    # "-C",
    # "link-arg=-s",
    # "-C",
    # "link-arg=FULL_ES3", # for opengl es 3 emulation
    # "-C",
    # "link-arg=-s",
    # "-C",
    # "link-arg=ERROR_ON_UNDEFINED_SYMBOLS=0", # for ignoring some egl symbols. maybe needed for wgpu 
    "-C",
    "link-arg=-s",
    "-C",
    "link-arg=MAX_WEBGL_VERSION=2 ", # to make sure that webgl2 is enabled. 
    "-C",
    "link-arg=-s",
    "-C",
    "link-arg=MIN_WEBGL_VERSION=2", # to disable webgl1 completely, and use webgl2 exclusively. 
    "-C",
    "link-arg=-s",
    "-C",
    "link-arg=DISABLE_DEPRECATED_FIND_EVENT_TARGET_BEHAVIOR=1", # i don't even remember why i have this :D.
]
```

canvas element's `data-raw-handle` property should be `1` and `id` should be `canvas`.

example html to use for your sdl2 wasm app:
```html
<!DOCTYPE html>
<html>
  <body>
    <canvas data-raw-handle="1" id="canvas"></canvas>
    <!-- you need this script to actually let sdl2 library find the canvas for backing its window -->
    <script type="text/javascript">
      var Module = {
        canvas: (function () {
          // this is how we provide a canvas to our sdl2
          return document.getElementById("canvas");
        })(),
      };
    </script>
    <!-- the above scrit MUST BE loaded before the below script which is generated by cargo.
    so, don't change the order of these tags -->
    <script src="my_project_name.js"></script>
  </body>
</html>
```
script to build and deploy
```sh
#!/bin/sh
echo "building for emscripten target"
# make sure that cargo use using the config file that has all the sdl2 linker options
cargo build --target=wasm32-unknown-emscripten --release
echo "copying files to dist directory"
# the directory can be anything temporary within which you want to place your server files
mkdir -p dist
# wasm file is obviously your rust binary
cp target/wasm32-unknown-emscripten/release/my_project_name.wasm dist
# but as the above binary needs to interact with browser via js, 
# this helper js script is generated by cargo to load and support the rust wasm binary
cp target/wasm32-unknown-emscripten/release/my_project_name.js dist
# just the above html file
cp index.html dist
# a temporary server to use for local development. DON'T USE IN PRODUCTION!!!
(cd dist && python -m http.server)
```

## Egui Overlay
In this context, Overlay simply means a gui application where
1. It has a transparent window surface
2. It can toggle the borders/decorations like title bar.
3. **can** let the input like mouse clicks to passthrough its window surface.

Here, we will let input passthrough when egui doesn't need input. 

The `egui_overlay` crate just combines the functionality from `egui_window_glfw_passthrough` for windowing functionality. 
For rendering, we use `egui_render_three_d`, as `three-d` will allow you to draw a bunch of things easily. 
But, as apple doesn't support opengl, we use `egui_render_wgpu` on macos.

For advanced usecases, i recommend directly using `egui_window_glfw_passthrough` crate directly with either wgpu or three-d or glow backend crates.
As you can see in `lib.rs`, its barely 150 lines of code to set up. It will allow you more control over event loop, as well as drawing.

Look at the `basic` example for a rough idea of how to use this crate for normal usecase.
Look at the `triangle` example (only for linux/windows users, as i use three-d), to see how you can draw custom stuff too.

> use `cargo run -p basic` to run the example.


https://github.com/coderedart/egui_overlay/assets/24411704/9f7bab7b-26ec-47d1-b51e-74006dfa7b0d

## Platforms
1. Windows
2. Linux 
    1. X11 supported natively. And wayland can work via Xwayland. But support might vary between window managers.
    2. You need a compositor that supports transparency. eg: `kwin` supports compositing, but `i3wm` needs an external compositor like `picom`. 
    3. Some tiling window managers like i3wm need users to configure your overlay window as "floating" to keep it above other tiled windows.  
3. Mac

## Bugs
1. On Mac, when passthrough is enabled, the window titlebar can only be clicked in the bottom half. The top half becomes passthrough too for some reason.
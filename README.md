## Egui Overlay
In this context, Overlay simply means a gui application where
1. It has a transparent window surface
2. It can toggle the borders/decorations like title bar.
3. **can** let the input like mouse clicks to passthrough its window surface.

Here, we will let input passthrough when egui doesn't need input. 

The project itself builds upon the `egui_backend` crate while using `egui_window_glfw_passthrough` for windowing functionality. 

For rendering, we use `egui_render_three_d`, as `three-d` will allow you to draw a bunch of things easily. 
But, as apple doesn't support opengl, we use `egui_render_wgpu` on macos.

For advanced usecases, i recommend directly using `egui_window_glfw_passthrough` from https://github.com/coderedart/etk

Look at the `basic` example for a rough idea of how to use this crate for normal usecase.

Look at the `triangle` example (only for linux/windows users), to see how you can draw custom stuff too.



https://github.com/coderedart/egui_overlay/assets/24411704/9f7bab7b-26ec-47d1-b51e-74006dfa7b0d

## Platforms
1. Windows
2. Linux (both X11 and Wayland). But support might vary between different window managers.
3. Mac


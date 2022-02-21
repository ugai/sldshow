
# sldshow

![CI](https://github.com/ugai/sldshow/actions/workflows/ci.yml/badge.svg)

![Icon](assets/icon/icon_32.png)

Simple slideshow image viewer.

- toml configuration/playlist file
- fixed-size, frameless window
- random transition effects

https://user-images.githubusercontent.com/3608128/134453085-19f2a90c-da6c-40a4-99d0-6e97ab4bd595.mp4

## Supported platforms

- Windows 64-bit

## Configuration file

sldshow can open a `.sldshow` file that is a [toml](https://toml.io/) format configurations containing image paths.

```toml
# sldshow file (.sldshow)

[window]
width = 1280
height = 780
fullscreen = false
always_on_top = false
titlebar = false
resizable = false # only when the titlebar is enabled
monitor_index = 0
cursor_auto_hide = true

[viewer]
image_paths = ["C:\\hoge\\dir1", 'C:\hoge\dir2', '/home/hoge/fuga.jpg']
timer = 10 # pause if value is zero
scan_subfolders = true
shuffle = true
pause_at_last = false
resize_filter = 'Linear' # ['Nearest', 'Linear', 'Cubic', 'Gaussian', 'Lanczos3']
stop_screensaver = true
cache_extent = 5 # preload the previous and next N files

[transition]
time = 0.5
fps = 30.0
random = true

[style]
bg_color = [0, 0, 0, 255] # RGBA [0, 255]
text_color = [255, 255, 255, 255] # RGBA [0, 255]
show_image_path = false
font_name = 'UD デジタル 教科書体 N-R'
font_size_osd = 18.0
font_size_image_path = 12.0
```

## Supported formats

sldshow uses [image-rs](https://crates.io/crates/image/).
It will probably be able to open the following image formats.

- PNG, JPEG, GIF, TIFF, TGA, BMP, ...

See image-rs [documentation](https://docs.rs/image/0.23/image/codecs/index.html#supported-formats) for details.

## Controls

| Action | Input |
|---|---|
| Quit | <kbd>Esc</kbd> / <kbd>q</kbd> / <kbd>MMB</kbd> |
| Next/previous image | <kbd>Right</kbd> and <kbd>Left</kbd> / <kbd>Down</kbd> and <kbd>Up</kbd> / <kbd>PageDown</kbd> and <kbd>PageUp</kbd> /<br/> <kbd>.</kbd> and <kbd>,</kbd> / <kbd>Enter</kbd> / <kbd>LMB</kbd> and <kbd>RMB</kbd> / <kbd>WheelDown</kbd> and <kbd>WheelUp</kbd> / <kbd>Tap</kbd> right or left side of the window |
| Next/previous 10th image | <kbd>Shift</kbd> +  Next/previous image |
| First image | <kbd>Home</kbd> |
| Last image | <kbd>End</kbd> |
| Toggle fullscreen | <kbd>f</kbd> / <kbd>F11</kbd> /  <kbd><kbd>Alt</kbd> + <kbd>Enter</kbd></kbd> / <kbd>Double-LMB</kbd> / <kbd>Tap (Multi-Finger)</kbd> |
| Minimize | <kbd><kbd>Alt</kbd> + <kbd>m</kbd></kbd> / <kbd><kbd>Alt</kbd> + <kbd>Down</kbd></kbd> |
| Toggle always on top | <kbd>t</kbd> |
| Toggle titlebar | <kbd>d</kbd> |
| Toggle pause/restart timer | <kbd>Space</kbd> / <kbd>p</kbd> |
| Pause timer | <kbd>Pause</kbd> |
| Decrease/increase display time | <kbd>[</kbd> and <kbd>]</kbd> |
| Reset display time | <kbd>Backspace</kbd> |
| Toggle pause/continue at last | <kbd>l</kbd> |
| Show current position | <kbd>o</kbd> |
| Copy current file path | <kbd><kbd>Ctrl</kbd> + <kbd>c</kbd></kbd> |
| Resize window to 50% | <kbd><kbd>Alt</kbd> + <kbd>0</kbd></kbd> |
| Resize window to 100% | <kbd><kbd>Alt</kbd> + <kbd>1</kbd></kbd> |
| Resize window to 200% | <kbd><kbd>Alt</kbd> + <kbd>2</kbd></kbd> |

## Alternatives

- [feh](https://feh.finalrewind.org/)
- [XnView](https://www.xnview.com/en/)
- [Simple Image Viewer](https://torum.github.io/Image-viewer/)
- [EMULSION](https://arturkovacs.github.io/emulsion-website/)

## License

MIT License

### About the icon

The icon of this application is derived from the "Photo" icon from the [Tabler Icons](https://github.com/tabler/tabler-icons). Copyright (c) 2020 Paweł Kuna, MIT Licensed.

### Transition shader

Some transition shader codes are based on the [GL Transitions](https://gl-transitions.com/).

- [randomsquares](https://gl-transitions.com/editor/randomsquares) - Author: gre, License: MIT
- [angular](https://gl-transitions.com/editor/angular) -  Author: gre, License: MIT

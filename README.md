# Demoengine

An OpenGL sandbox for rapid prototyping.

It is currently in a usable state, but by no means feature complete. APIs and DSL may be subject to change. Pull requests and contributions are welcome.

### Prerequisites

A `rocket` fronted needs to be up and running before starting the application. This will enable you to adjust and animate different parameters. Some are [GNU Rocket](https://github.com/rocket/rocket) or [RocketEditor](https://github.com/emoon/rocket).

### Running

The entry point for the engine is a script file with the `.demo` extension. This file is written in a domain specific language, and allows you to execute many of the OpenGL operations, loading scripts, defining render targets, issuing drawcalls, configuring the rendering pipeline, interfacing with the rocket frontend, and much more.

In order to execute a `.demo` file you can issue the following command:

    $ cargo run --release -- examples/hello_world/main.demo

After this, the engine will start running the demo. Any errors are reported to the console. Furthermore, the engine listens for file changes and will automatically reload the demo when anything changes.


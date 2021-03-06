#![feature(plugin)]
#![plugin(rocket_codegen)]

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate serde_derive;

use clap::{App, Arg};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use dotenv::dotenv;
use rocket_contrib::Json;
use std::env;
use std::fs::File;
use std::io::prelude::*;

use crate::config::Config;

mod config;
pub mod models;
pub mod schema;

use crate::models::*;
extern crate rand;
#[macro_use]
extern crate vulkano;
use vulkano::instance::Instance;
use vulkano::instance::InstanceExtensions;
use vulkano::instance::PhysicalDevice;
use vulkano_win::VkSurfaceBuild;

use std::sync::Arc;
use std::thread;
use vulkano::buffer::BufferUsage;
use vulkano::buffer::CpuAccessibleBuffer;
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::command_buffer::DynamicState;
use vulkano::device::Device;
use vulkano::framebuffer::Framebuffer;
use vulkano::framebuffer::Subpass;
use vulkano::pipeline::viewport::Viewport;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::swapchain;
use vulkano::swapchain::AcquireError;
use vulkano::swapchain::PresentMode;
use vulkano::swapchain::SurfaceTransform;
use vulkano::swapchain::Swapchain;
use vulkano::swapchain::SwapchainCreationError;
use vulkano::sync::now;
use vulkano::sync::GpuFuture;

// The `vulkano_shader_derive` crate allows us to use the `VulkanoShader` custom derive that we use
// in this example.
#[macro_use]
extern crate vulkano_shader_derive;
// However the Vulkan library doesn't provide any functionality to create and handle windows, as
// this would be out of scope. In order to open a window, we are going to use the `winit` crate.
extern crate winit;
// The `vulkano_win` crate is the link between `vulkano` and `winit`. Vulkano doesn't know about
// winit, and winit doesn't know about vulkano, so import a crate that will provide a link between
// the two.
extern crate vulkano_win;

#[get("/<name>/<age>")]
fn hello(name: String, age: u8) -> String {
    format!("Hello, {} year old named {}!", age, name)
}

#[post("/shit", format = "application/json", data = "<something>")]
fn shit(something: Json<Something>) -> String {
    let child = thread::spawn(move || {
        let instance = Instance::new(None, &InstanceExtensions::none(), None)
            .expect("failed to create instance");

        let physical = PhysicalDevice::enumerate(&instance)
            .next()
            .expect("no device available");

        // The first step of any Vulkan program is to create an instance.
        let instance = {
            // When we create an instance, we have to pass a list of extensions that we want to enable.
            //
            // All the window-drawing functionalities are part of non-core extensions that we need
            // to enable manually. To do so, we ask the `vulkano_win` crate for the list of extensions
            // required to draw to a window.
            let extensions = vulkano_win::required_extensions();

            // Now creating the instance.
            Instance::new(None, &extensions, None).expect("failed to create Vulkan instance")
        };

        // We then choose which physical device to use.
        //
        // In a real application, there are three things to take into consideration:
        //
        // - Some devices may not support some of the optional features that may be required by your
        //   application. You should filter out the devices that don't support your app.
        //
        // - Not all devices can draw to a certain surface. Once you create your window, you have to
        //   choose a device that is capable of drawing to it.
        //
        // - You probably want to leave the choice between the remaining devices to the user.
        //
        // For the sake of the example we are just going to use the first device, which should work
        // most of the time.
        let physical = vulkano::instance::PhysicalDevice::enumerate(&instance)
            .next()
            .expect("no device available");
        // Some little debug infos.
        println!(
            "Using device: {} (type: {:?})",
            physical.name(),
            physical.ty()
        );

        // The objective of this example is to draw a triangle on a window. To do so, we first need to
        // create the window.
        //
        // This is done by creating a `WindowBuilder` from the `winit` crate, then calling the
        // `build_vk_surface` method provided by the `VkSurfaceBuild` trait from `vulkano_win`. If you
        // ever get an error about `build_vk_surface` being undefined in one of your projects, this
        // probably means that you forgot to import this trait.
        //
        // This returns a `vulkano::swapchain::Surface` object that contains both a cross-platform winit
        // window and a cross-platform Vulkan surface that represents the surface of the window.
        let mut events_loop = winit::EventsLoop::new();
        let surface = winit::WindowBuilder::new()
            .build_vk_surface(&events_loop, instance.clone())
            .unwrap();

        // The next step is to choose which GPU queue will execute our draw commands.
        //
        // Devices can provide multiple queues to run commands in parallel (for example a draw queue
        // and a compute queue), similar to CPU threads. This is something you have to have to manage
        // manually in Vulkan.
        //
        // In a real-life application, we would probably use at least a graphics queue and a transfers
        // queue to handle data transfers in parallel. In this example we only use one queue.
        //
        // We have to choose which queues to use early on, because we will need this info very soon.
        let queue_family = physical
            .queue_families()
            .find(|&q| {
                // We take the first queue that supports drawing to our window.
                q.supports_graphics() && surface.is_supported(q).unwrap_or(false)
            })
            .expect("couldn't find a graphical queue family");

        // Now initializing the device. This is probably the most important object of Vulkan.
        //
        // We have to pass five parameters when creating a device:
        //
        // - Which physical device to connect to.
        //
        // - A list of optional features and extensions that our program needs to work correctly.
        //   Some parts of the Vulkan specs are optional and must be enabled manually at device
        //   creation. In this example the only thing we are going to need is the `khr_swapchain`
        //   extension that allows us to draw to a window.
        //
        // - A list of layers to enable. This is very niche, and you will usually pass `None`.
        //
        // - The list of queues that we are going to use. The exact parameter is an iterator whose
        //   items are `(Queue, f32)` where the floating-point represents the priority of the queue
        //   between 0.0 and 1.0. The priority of the queue is a hint to the implementation about how
        //   much it should prioritize queues between one another.
        //
        // The list of created queues is returned by the function alongside with the device.
        let (device, mut queues) = {
            let device_ext = vulkano::device::DeviceExtensions {
                khr_swapchain: true,
                ..vulkano::device::DeviceExtensions::none()
            };

            Device::new(
                physical,
                physical.supported_features(),
                &device_ext,
                [(queue_family, 0.5)].iter().cloned(),
            )
            .expect("failed to create device")
        };

        // Since we can request multiple queues, the `queues` variable is in fact an iterator. In this
        // example we use only one queue, so we just retrieve the first and only element of the
        // iterator and throw it away.
        let queue = queues.next().unwrap();

        // The dimensions of the surface.
        // This variable needs to be mutable since the viewport can change size.
        let mut dimensions;

        // Before we can draw on the surface, we have to create what is called a swapchain. Creating
        // a swapchain allocates the color buffers that will contain the image that will ultimately
        // be visible on the screen. These images are returned alongside with the swapchain.
        let (mut swapchain, mut images) = {
            // Querying the capabilities of the surface. When we create the swapchain we can only
            // pass values that are allowed by the capabilities.
            let caps = surface
                .capabilities(physical)
                .expect("failed to get surface capabilities");

            dimensions = caps.current_extent.unwrap_or([1024, 768]);

            // We choose the dimensions of the swapchain to match the current extent of the surface.
            // If `caps.current_extent` is `None`, this means that the window size will be determined
            // by the dimensions of the swapchain, in which case we just use the width and height defined above.

            // The alpha mode indicates how the alpha value of the final image will behave. For example
            // you can choose whether the window will be opaque or transparent.
            let alpha = caps.supported_composite_alpha.iter().next().unwrap();

            // Choosing the internal format that the images will have.
            let format = caps.supported_formats[0].0;

            // Please take a look at the docs for the meaning of the parameters we didn't mention.
            Swapchain::new(
                device.clone(),
                surface.clone(),
                caps.min_image_count,
                format,
                dimensions,
                1,
                caps.supported_usage_flags,
                &queue,
                SurfaceTransform::Identity,
                alpha,
                PresentMode::Fifo,
                true,
                None,
            )
            .expect("failed to create swapchain")
        };

        // We now create a buffer that will store the shape of our triangle.
        let vertex_buffer = {
            #[derive(Debug, Clone)]
            struct Vertex {
                position: [f32; 2],
            }
            impl_vertex!(Vertex, position);

            CpuAccessibleBuffer::from_iter(
                device.clone(),
                BufferUsage::all(),
                [
                    Vertex {
                        position: [-0.5, -0.25],
                    },
                    Vertex {
                        position: [0.0, 0.5],
                    },
                    Vertex {
                        position: [0.25, -0.1],
                    },
                ]
                    .iter()
                    .cloned(),
            )
            .expect("failed to create buffer")
        };

        // The next step is to create the shaders.
        //
        // The raw shader creation API provided by the vulkano library is unsafe, for various reasons.
        //
        // An overview of what the `VulkanoShader` derive macro generates can be found in the
        // `vulkano-shader-derive` crate docs. You can view them at
        // https://docs.rs/vulkano-shader-derive/*/vulkano_shader_derive/
        //
        // TODO: explain this in details
        mod vs {
            #[derive(VulkanoShader)]
            #[ty = "vertex"]
            #[src = "
#version 450

layout(location = 0) in vec2 position;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
}
"]
            #[allow(dead_code)]
            struct Dummy;
        }

        mod fs {
            #[derive(VulkanoShader)]
            #[ty = "fragment"]
            #[src = "
#version 450

layout(location = 0) out vec4 f_color;

void main() {
    f_color = vec4(1.0, 0.0, 0.0, 1.0);
}
"]
            #[allow(dead_code)]
            struct Dummy;
        }

        let vs = vs::Shader::load(device.clone()).expect("failed to create shader module");
        let fs = fs::Shader::load(device.clone()).expect("failed to create shader module");

        // At this point, OpenGL initialization would be finished. However in Vulkan it is not. OpenGL
        // implicitly does a lot of computation whenever you draw. In Vulkan, you have to do all this
        // manually.

        // The next step is to create a *render pass*, which is an object that describes where the
        // output of the graphics pipeline will go. It describes the layout of the images
        // where the colors, depth and/or stencil information will be written.
        let render_pass = Arc::new(
            single_pass_renderpass!(device.clone(),
        attachments: {
            // `color` is a custom name we give to the first and only attachment.
            color: {
                // `load: Clear` means that we ask the GPU to clear the content of this
                // attachment at the start of the drawing.
                load: Clear,
                // `store: Store` means that we ask the GPU to store the output of the draw
                // in the actual image. We could also ask it to discard the result.
                store: Store,
                // `format: <ty>` indicates the type of the format of the image. This has to
                // be one of the types of the `vulkano::format` module (or alternatively one
                // of your structs that implements the `FormatDesc` trait). Here we use the
                // generic `vulkano::format::Format` enum because we don't know the format in
                // advance.
                format: swapchain.format(),
                // TODO:
                samples: 1,
            }
        },
        pass: {
            // We use the attachment named `color` as the one and only color attachment.
            color: [color],
            // No depth-stencil attachment is indicated with empty brackets.
            depth_stencil: {}
        }
    )
            .unwrap(),
        );

        // Before we draw we have to create what is called a pipeline. This is similar to an OpenGL
        // program, but much more specific.
        let pipeline = Arc::new(
            GraphicsPipeline::start()
                // We need to indicate the layout of the vertices.
                // The type `SingleBufferDefinition` actually contains a template parameter corresponding
                // to the type of each vertex. But in this code it is automatically inferred.
                .vertex_input_single_buffer()
                // A Vulkan shader can in theory contain multiple entry points, so we have to specify
                // which one. The `main` word of `main_entry_point` actually corresponds to the name of
                // the entry point.
                .vertex_shader(vs.main_entry_point(), ())
                // The content of the vertex buffer describes a list of triangles.
                .triangle_list()
                // Use a resizable viewport set to draw over the entire window
                .viewports_dynamic_scissors_irrelevant(1)
                // See `vertex_shader`.
                .fragment_shader(fs.main_entry_point(), ())
                // We have to indicate which subpass of which render pass this pipeline is going to be used
                // in. The pipeline will only be usable from this particular subpass.
                .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
                // Now that our builder is filled, we call `build()` to obtain an actual pipeline.
                .build(device.clone())
                .unwrap(),
        );

        // The render pass we created above only describes the layout of our framebuffers. Before we
        // can draw we also need to create the actual framebuffers.
        //
        // Since we need to draw to multiple images, we are going to create a different framebuffer for
        // each image.
        let mut framebuffers: Option<Vec<Arc<vulkano::framebuffer::Framebuffer<_, _>>>> = None;

        // Initialization is finally finished!

        // In some situations, the swapchain will become invalid by itself. This includes for example
        // when the window is resized (as the images of the swapchain will no longer match the
        // window's) or, on Android, when the application went to the background and goes back to the
        // foreground.
        //
        // In this situation, acquiring a swapchain image or presenting it will return an error.
        // Rendering to an image of that swapchain will not produce any error, but may or may not work.
        // To continue rendering, we need to recreate the swapchain by creating a new swapchain.
        // Here, we remember that we need to do this for the next loop iteration.
        let mut recreate_swapchain = false;

        // In the loop below we are going to submit commands to the GPU. Submitting a command produces
        // an object that implements the `GpuFuture` trait, which holds the resources for as long as
        // they are in use by the GPU.
        //
        // Destroying the `GpuFuture` blocks until the GPU is finished executing it. In order to avoid
        // that, we store the submission of the previous frame here.
        let mut previous_frame_end = Box::new(now(device.clone())) as Box<GpuFuture>;

        let mut dynamic_state = DynamicState {
            line_width: None,
            viewports: Some(vec![Viewport {
                origin: [0.0, 0.0],
                dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                depth_range: 0.0..1.0,
            }]),
            scissors: None,
        };

        let mut running = true;

        while (running) {
            // It is important to call this function from time to time, otherwise resources will keep
            // accumulating and you will eventually reach an out of memory error.
            // Calling this function polls various fences in order to determine what the GPU has
            // already processed, and frees the resources that are no longer needed.
            previous_frame_end.cleanup_finished();

            // If the swapchain needs to be recreated, recreate it
            if recreate_swapchain {
                // Get the new dimensions for the viewport/framebuffers.
                dimensions = surface
                    .capabilities(physical)
                    .expect("failed to get surface capabilities")
                    .current_extent
                    .unwrap();

                let (new_swapchain, new_images) =
                    match swapchain.recreate_with_dimension(dimensions) {
                        Ok(r) => r,
                        // This error tends to happen when the user is manually resizing the window.
                        // Simply restarting the loop is the easiest way to fix this issue.
                        Err(SwapchainCreationError::UnsupportedDimensions) => {
                            continue;
                        }
                        Err(err) => panic!("{:?}", err),
                    };

                swapchain = new_swapchain;
                images = new_images;

                framebuffers = None;

                dynamic_state.viewports = Some(vec![Viewport {
                    origin: [0.0, 0.0],
                    dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                    depth_range: 0.0..1.0,
                }]);

                recreate_swapchain = false;
            }

            // Because framebuffers contains an Arc on the old swapchain, we need to
            // recreate framebuffers as well.
            if framebuffers.is_none() {
                framebuffers = Some(
                    images
                        .iter()
                        .map(|image| {
                            Arc::new(
                                Framebuffer::start(render_pass.clone())
                                    .add(image.clone())
                                    .unwrap()
                                    .build()
                                    .unwrap(),
                            )
                        })
                        .collect::<Vec<_>>(),
                );
            }

            // Before we can draw on the output, we have to *acquire* an image from the swapchain. If
            // no image is available (which happens if you submit draw commands too quickly), then the
            // function will block.
            // This operation returns the index of the image that we are allowed to draw upon.
            //
            // This function can block if no image is available. The parameter is an optional timeout
            // after which the function call will return an error.
            let (image_num, acquire_future) =
                match swapchain::acquire_next_image(swapchain.clone(), None) {
                    Ok(r) => r,
                    Err(AcquireError::OutOfDate) => {
                        recreate_swapchain = true;
                        continue;
                    }
                    Err(err) => panic!("{:?}", err),
                };

            // In order to draw, we have to build a *command buffer*. The command buffer object holds
            // the list of commands that are going to be executed.
            //
            // Building a command buffer is an expensive operation (usually a few hundred
            // microseconds), but it is known to be a hot path in the driver and is expected to be
            // optimized.
            //
            // Note that we have to pass a queue family when we create the command buffer. The command
            // buffer will only be executable on that given queue family.
            let command_buffer =
                AutoCommandBufferBuilder::primary_one_time_submit(device.clone(), queue.family())
                    .unwrap()
                    // Before we can draw, we have to *enter a render pass*. There are two methods to do
                    // this: `draw_inline` and `draw_secondary`. The latter is a bit more advanced and is
                    // not covered here.
                    //
                    // The third parameter builds the list of values to clear the attachments with. The API
                    // is similar to the list of attachments when building the framebuffers, except that
                    // only the attachments that use `load: Clear` appear in the list.
                    .begin_render_pass(
                        framebuffers.as_ref().unwrap()[image_num].clone(),
                        false,
                        vec![[0.0, 0.0, 1.0, 1.0].into()],
                    )
                    .unwrap()
                    // We are now inside the first subpass of the render pass. We add a draw command.
                    //
                    // The last two parameters contain the list of resources to pass to the shaders.
                    // Since we used an `EmptyPipeline` object, the objects have to be `()`.
                    .draw(
                        pipeline.clone(),
                        &dynamic_state,
                        vertex_buffer.clone(),
                        (),
                        (),
                    )
                    .unwrap()
                    // We leave the render pass by calling `draw_end`. Note that if we had multiple
                    // subpasses we could have called `next_inline` (or `next_secondary`) to jump to the
                    // next subpass.
                    .end_render_pass()
                    .unwrap()
                    // Finish building the command buffer by calling `build`.
                    .build()
                    .unwrap();

            let future = previous_frame_end
                .join(acquire_future)
                .then_execute(queue.clone(), command_buffer)
                .unwrap()
                // The color output is now expected to contain our triangle. But in order to show it on
                // the screen, we have to *present* the image by calling `present`.
                //
                // This function does not actually present the image immediately. Instead it submits a
                // present command at the end of the queue. This means that it will only be presented once
                // the GPU has finished executing the command buffer that draws the triangle.
                .then_swapchain_present(queue.clone(), swapchain.clone(), image_num)
                .then_signal_fence_and_flush();

            match future {
                Ok(future) => {
                    previous_frame_end = Box::new(future) as Box<_>;
                }
                Err(vulkano::sync::FlushError::OutOfDate) => {
                    recreate_swapchain = true;
                    previous_frame_end = Box::new(vulkano::sync::now(device.clone())) as Box<_>;
                }
                Err(e) => {
                    println!("{:?}", e);
                    previous_frame_end = Box::new(vulkano::sync::now(device.clone())) as Box<_>;
                }
            }

            // Note that in more complex programs it is likely that one of `acquire_next_image`,
            // `command_buffer::submit`, or `present` will block for some time. This happens when the
            // GPU's queue is full and the driver has to wait until the GPU finished some work.
            //
            // Unfortunately the Vulkan API doesn't provide any way to not wait or to detect when a
            // wait would happen. Blocking may be the desired behavior, but if you don't want to
            // block you should spawn a separate thread dedicated to submissions.

            // Handling the window events in order to close the program when the user wants to close
            // it.
            use winit::{ControlFlow, Event, WindowEvent};

            events_loop.run_forever(|ev| match ev {
                winit::Event::WindowEvent {
                    event: winit::WindowEvent::CloseRequested,
                    ..
                } => {
                    running = false;
                    println!("something herer");
                    ControlFlow::Break
                }
                _ => ControlFlow::Continue,
            });

            println!("hey its me still");
        }
    });

    println!("waiting to join");
    let res = child.join();

    format!("hey its a device {}", something.turd)
}

#[derive(Deserialize)]
pub struct Something {
    turd: String,
}

fn main() {
    use crate::schema::posts::dsl::*;
    dotenv().ok();

    println!("hey its me");

    let matches = App::new("this shit")
        .version("1.0")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("shit")
                .short("s")
                .long("shit")
                .value_name("something_here")
                .help("Sets a custom config file")
                .takes_value(true),
        )
        .get_matches();

    let filename: &str = "config.toml";
    let mut f = File::open(filename).expect("file not found");
    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("shit brah something went wrong");

    // TODO: dont use unwrap
    let config: Config = toml::from_str(&contents.to_owned()).unwrap();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let connection = PgConnection::establish(&database_url)
        .expect(&format!("Error connecting to {}", database_url));

    println!("{}", contents);

    let results = posts
        .filter(published.eq(true))
        .limit(5)
        .load::<Post>(&connection)
        .expect("Error loading posts");

    println!("Displaying {} posts", results.len());
    for post in results {
        println!("{}", post.title);
        println!("----------\n");
        println!("{}", post.body);
    }

    rocket::ignite()
        .mount("/hello", routes![hello, shit])
        .launch();
}

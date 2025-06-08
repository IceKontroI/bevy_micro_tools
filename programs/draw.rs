use bevy::{asset::*, ecs::query::*, image::*, input::mouse::*, math::*, prelude::*};
use bevy::render::{extract_resource::*, render_graph::*, render_resource::*, renderer::*, view::*, *};
use bevy::core_pipeline::core_2d::graph::*;
use chain_link::{Length, L};
use extract_component::*;
use ndex::{Index, IndexMut};
use crate::{*, attach::*, wgputil::*};

// TODO this is almost set up to work with multiple views, but not quite compatible yet
//      we need a per-camera MouseDrawing component, not a global MouseDrawing resource

const MIN_BRUSH_SIZE: f32 = 8.0;

pub struct DrawPlugin;

impl Plugin for DrawPlugin {

    fn build(&self, app: &mut App) {

        // core for generating the mouse trail and inputs to the draw shader
        app.insert_resource(MouseDrawing { min_brush_size: MIN_BRUSH_SIZE, ..default() });
        app.add_plugins(ExtractResourcePlugin::<MouseDrawing>::default());
        app.add_systems(Update, mouse_drawing_system);

        // required for auto-resizing the draw canvas
        // we can't use the screen output as canvas since it's not persistent
        app.add_plugins(AttachPlugin::<DrawCanvas, AndExtract>::default());

        // create a 2d camera with the DrawCanvas component, which will be automatically resized for us
        app.add_systems(Startup, |mut commands: Commands| {
            commands.spawn((
                DrawCanvas::default(), // our extractable auto-resizing attachment image(s)
                Camera2d::default(), // the camera which will serve as our view target
            ));
        });

        // initialize the custom render passes that let us draw to the screen
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_render_graph_node::<ViewNodeRunner<DrawCanvasPass>>(Core2d, DrawCanvasPass);
            render_app.add_render_graph_node::<ViewNodeRunner<Passthrough>>(Core2d, Passthrough);
            render_app.add_render_graph_edges(Core2d, (
                Node2d::StartMainPass,
                DrawCanvasPass, // this will add the trail increment to the persistent DrawCanvas
                Node2d::Tonemapping, 
                Passthrough, // this copies that canvas over to the actual screen render target
                Node2d::EndMainPassPostProcessing,
            ));
        };
    }

    fn finish(&self, app: &mut App) {
        // initialize the custom render pass pipelines
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_resource::<RasterPipeline<DrawCanvasPass>>();
            render_app.init_resource::<RasterPipeline<Passthrough>>();
        }
    }
}

// auto-resizing image attachment, which also doubles as a bind group impl
#[derive(Index, IndexMut, Component, Default, Clone, ExtractComponent, AsBindGroup)]
pub struct DrawCanvas {
    #[index(0)]
    #[texture(0, filterable = false, visibility(all))]
    handle: Handle<Image>,
}

// since the Attachment<N> trait requires Index<usize, Output = Handle<Image>>
// we just need to implement Attachment at the specific index of our Handle
// and it works seamlessly with bevy's AsBindGroup macro

impl Attach<0> for DrawCanvas {
    const COPY_ON_RESIZE: bool = true;
    const TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba32Float;
    const TEXTURE_ASPECT: TextureAspect = TextureAspect::All;
    const TEXTURE_USAGES: TextureUsages = TextureUsages::RENDER_ATTACHMENT
        .union(TextureUsages::TEXTURE_BINDING)
        .union(TextureUsages::COPY_SRC)
        .union(TextureUsages::COPY_DST);
}

// only way chaining (currently) is able to work is if we have a single length impl
// for the type which we want to cascade the chain-link for
// TODO make a proc macro for this and integrate it with Index and IndexMut macros
impl Length for DrawCanvas {
    // TODO L<N> is suboptimal API syntax, but it's the cleanest workaround to the
    //      rust compiler blindspot that misses non-overlapping trait impls that are
    //      conditional on associated type equality
    type Len = L<1>;
}

// resource for tracking mouse trail and submitting quads to vertex shader
#[derive(Resource, Default, Clone, ExtractResource)]
pub struct MouseDrawing {
    pub min_brush_size: f32,
    pub radius: f32,
    pub last_quad: Option<[Vec4; 4]>, // Quad vertices in NDC
    pub is_drawing: bool,
    pub last_pos: Option<Vec2>,   // Position in pixel coordinates
    pub last_left: Option<Vec2>,  // Left edge in pixel coordinates
    pub last_right: Option<Vec2>, // Right edge in pixel coordinates
    pub continuation: bool,
    pub brush_type: u32,
    pub smoothed_direction: Option<Vec2>, // Smoothed direction for smoother trails
}

// different supported brush types
pub enum BrushType {
    Erase = 0,
    Draw = 1,
}

pub fn mouse_drawing_system(
    camera: Query<&Camera>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut mouse_moved: EventReader<CursorMoved>,
    mut mouse_wheel: EventReader<MouseWheel>,
    mut mouse_trail: ResMut<MouseDrawing>,
) {
    // Adjust brush radius with mouse wheel
    let wheel_delta: f32 = mouse_wheel.read().map(|wheel| wheel.x + wheel.y).sum();
    mouse_trail.radius = f32::max(mouse_trail.min_brush_size, mouse_trail.radius + wheel_delta);

    // Handle mouse button input
    if mouse.pressed(MouseButton::Left) {
        mouse_trail.brush_type = BrushType::Draw as u32;
    } else if mouse.pressed(MouseButton::Right) {
        mouse_trail.brush_type = BrushType::Erase as u32;
    } else {
        mouse_trail.is_drawing = false;
        mouse_trail.last_pos = None;
        mouse_trail.last_quad = None;
        mouse_trail.last_left = None;
        mouse_trail.last_right = None;
        mouse_trail.continuation = false;
        mouse_trail.smoothed_direction = None;
        return;
    }

    // Get viewport dimensions from camera
    let camera = camera.single().expect("No camera found");
    let viewport_size = camera.physical_viewport_size().expect("No viewport size");
    let w = viewport_size.x as f32;
    let h = viewport_size.y as f32;

    // Get latest mouse position
    let xy = match mouse_moved.read().last() {
        Some(last_move) => last_move.position,
        None => {
            mouse_trail.last_quad = None;
            return;
        }
    };

    // Convert pixel coordinates to NDC
    let to_ndc = |p: Vec2| -> Vec2 {
        Vec2 {
            x: 2.0 * (p.x / w) - 1.0,
            y: 1.0 - 2.0 * (p.y / h),
        }
    };

    if let Some(prev_pos) = mouse_trail.last_pos {
        mouse_trail.continuation = true;
        let direction_pixels = xy - prev_pos;
        if direction_pixels.length_squared() > 0.0 {
            let current_direction = direction_pixels.normalize();
            let smoothed_direction = mouse_trail.smoothed_direction.map_or(
                current_direction,
                |prev| (prev * 0.5 + current_direction * 0.5).normalize()
            );
            mouse_trail.smoothed_direction = Some(smoothed_direction);
            let perp_pixels = Vec2::new(-smoothed_direction.y, smoothed_direction.x) * mouse_trail.radius;
            let current_left = xy + perp_pixels;
            let current_right = xy - perp_pixels;
            let last_left_val = mouse_trail.last_left.unwrap_or(prev_pos + perp_pixels);
            let last_right_val = mouse_trail.last_right.unwrap_or(prev_pos - perp_pixels);

            // Determine if we need to swap vertices to connect to the front-most edge
            let prev_direction = mouse_trail.smoothed_direction.unwrap_or(current_direction);
            let dot_product = current_direction.dot(prev_direction);
            let (connect_left, connect_right) = if dot_product < 0.0 {
                // Sharp turn detected, swap connections to avoid twisting
                (last_right_val, last_left_val)
            } else {
                (last_left_val, last_right_val)
            };

            mouse_trail.last_quad = Some([
                to_ndc(connect_left).extend(0.0).extend(1.0),   // Connect to previous left or right
                to_ndc(connect_right).extend(0.0).extend(1.0),  // Connect to previous right or left
                to_ndc(current_left).extend(0.0).extend(1.0),   // Current left
                to_ndc(current_right).extend(0.0).extend(1.0),  // Current right
            ]);
            mouse_trail.last_left = Some(current_left);
            mouse_trail.last_right = Some(current_right);
            mouse_trail.is_drawing = true;
        } else {
            mouse_trail.last_quad = None;
        }
    } else {
        mouse_trail.continuation = false;
        // Initial position: create a small square
        let r = mouse_trail.radius;
        let pos_00 = xy + Vec2::new(-r, -r);
        let pos_10 = xy + Vec2::new( r, -r);
        let pos_01 = xy + Vec2::new(-r,  r);
        let pos_11 = xy + Vec2::new( r,  r);
        mouse_trail.last_quad = Some([
            to_ndc(pos_00).extend(0.0).extend(1.0),
            to_ndc(pos_10).extend(0.0).extend(1.0),
            to_ndc(pos_01).extend(0.0).extend(1.0),
            to_ndc(pos_11).extend(0.0).extend(1.0),
        ]);
        mouse_trail.is_drawing = false;
        mouse_trail.smoothed_direction = None;
    }

    mouse_trail.last_pos = Some(xy);
}

// params passed to the draw.wgsl shader
#[derive(Default, Copy, Clone, ShaderType)]
pub struct DrawParams {
    quad: [Vec4; 4], // next quad in the mouse trail
    brush: u32, // which brush is currently selected
}

define_render_pass_struct!(DrawCanvasPass);

impl Pass for DrawCanvasPass {
    type Binds = (Uniform<DrawParams>,);
}

impl Raster for DrawCanvasPass {
    const VERTEX_FRAGMENT_SHADER_PATH: &'static str = "shaders/draw.wgsl";

    fn fragment_targets() -> Vec<Option<ColorTargetState>> {
        vec![Some(DrawCanvas::color_target_state())] 
    }
}

impl ViewNode for DrawCanvasPass {

    type ViewQuery = &'static DrawCanvas;

    fn run(
        &self, 
        _: &mut RenderGraphContext, 
        context: &mut RenderContext, 
        canvas: QueryItem<Self::ViewQuery>, 
        world: &World
    ) -> Result<(), NodeRunError> {

        // TODO this could be a view on the camera and we'll just render per-camera and it'll be great and ...
        let MouseDrawing { 
            continuation: true, 
            last_quad: Some(quad),
            brush_type,
            .. 
        } = world.resource::<MouseDrawing>() else {
            return Ok(());
        };
        let uniform = DrawParams {
            quad: *quad,
            brush: *brush_type,
        }.into_uniform();
        
        let pipelines = world.resource::<PipelineCache>();
        let draw_pipeline = world.resource::<RasterPipeline<Self>>();
        let Some(pipeline) = pipelines.get_render_pipeline(draw_pipeline.id()) else { 
            warn!("Missing Draw Pipeline");
            return Ok(());
        };

        let device = context.render_device();
        let params = &mut get_binding_group_params(world);
        let group0 = uniform.as_bind_group(&draw_pipeline[0], device, params).unwrap().bind_group;

        let Some(canvas) = params.0.get(&canvas[0]) else {
            warn!("Missing DrawCanvas GPU Image");
            return Ok(());
        };
        let canvas = RenderPassColorAttachment {
            view: &canvas.texture.create_view(&DrawCanvas::texture_view(canvas.size).descriptor()),
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Load,
                store: StoreOp::Store
            }
        };

        let descriptor = RenderPassDescriptor {
            label: Some("Draw Pass"),
            color_attachments: &[Some(canvas)],
            depth_stencil_attachment: None,
            ..default()
        };
        let mut render_pass = context.command_encoder().begin_render_pass(&descriptor);
        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, &group0, &[]);
        render_pass.draw(0..4, 0..1);

        Ok(())
    }
}

define_render_pass_struct!(Passthrough);

impl Pass for Passthrough {
    type Binds = (DrawCanvas,);
}

// TODO make separate vertex shader for fullscreen
//      and then decouple the shader requirement so we can just point to that fullscreen shader
//      or better yet use bevy's built-in fullscreen post process vertex shader
impl Raster for Passthrough {
    const VERTEX_FRAGMENT_SHADER_PATH: &'static str = "shaders/passthrough.wgsl";

    fn fragment_targets() -> Vec<Option<ColorTargetState>> {
        vec![Some(TextureFormat::bevy_default().into())] 
    }
}

impl ViewNode for Passthrough {

    type ViewQuery = (
        &'static ViewTarget,
        &'static DrawCanvas,
    );

    fn run(
        &self, 
        _: &mut RenderGraphContext, 
        context: &mut RenderContext, 
        (view, canvas): QueryItem<Self::ViewQuery>, 
        world: &World
    ) -> Result<(), NodeRunError> {
        
        let pipelines = world.resource::<PipelineCache>();
        let passthrough_pipeline = world.resource::<RasterPipeline<Self>>();
        let Some(pipeline) = pipelines.get_render_pipeline(passthrough_pipeline.id()) else { 
            warn!("Missing Passthrough Pipeline");
            return Ok(());
        };

        let device = context.render_device();
        let params = &mut get_binding_group_params(world);
        let Ok(group0) = canvas.as_bind_group(&passthrough_pipeline[0], device, params) else {
            warn!("Missing???");
            return Ok(());
        };

        let post_process = view.post_process_write();
        let attachment = RenderPassColorAttachment {
            view: post_process.destination,
            resolve_target: None,
            ops: default(),
        };

        let descriptor = RenderPassDescriptor {
            label: Some("Passthrough"),
            color_attachments: &[Some(attachment)],
            depth_stencil_attachment: None,
            ..default()
        };
        let mut render_pass = context.command_encoder().begin_render_pass(&descriptor);
        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, &group0.bind_group, &[]);
        render_pass.draw(0..4, 0..1);

        Ok(())
    }
}

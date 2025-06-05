use std::any::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::storage::GpuShaderStorageBuffer;
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::{asset::*, log::*, prelude::*};
use bevy::render::{render_resource::*, renderer::*};
use encase::internal::WriteInto;

// TODO this is garbage, clean it up

#[macro_export]
macro_rules! define_render_pass_struct {
    ($name:ident) => {
        #[derive(
            core::default::Default, 
            core::fmt::Debug, 
            core::marker::Copy, 
            core::clone::Clone, 
            core::hash::Hash, 
            core::cmp::PartialEq, 
            core::cmp::Eq, 
            bevy::render::render_graph::RenderLabel
        )]    
        pub struct $name;
    };
}

pub trait Binds {
    type Layout: Clone + Into<Vec<BindGroupLayout>>;
    
    fn into_layout(device: &RenderDevice) -> Self::Layout;
}

macro_rules! impl_binds {
    () => {
        impl Binds for () {
            type Layout = [BindGroupLayout; 0];
            fn into_layout(_: &RenderDevice) -> Self::Layout { [] }
        }
    };
    ($len:expr; $($T:ident => $idx:tt),+ $(,)?) => {
        impl< $( $T: AsBindGroup ),+ > Binds for ( $( $T ),+, ) {
            type Layout = [BindGroupLayout; $len];
            fn into_layout(device: &RenderDevice) -> Self::Layout {
                [$(<$T as AsBindGroup>::bind_group_layout(device)),+]
            }
        }
    };
}

impl_binds!();
impl_binds!(1; A => 0);
impl_binds!(2; A => 0, B => 1);
impl_binds!(3; A => 0, B => 1, C => 2);
impl_binds!(4; A => 0, B => 1, C => 2, D => 3);
impl_binds!(5; A => 0, B => 1, C => 2, D => 3, E => 4);
impl_binds!(6; A => 0, B => 1, C => 2, D => 3, E => 4, F => 5);
impl_binds!(7; A => 0, B => 1, C => 2, D => 3, E => 4, F => 5, G => 6);
impl_binds!(8; A => 0, B => 1, C => 2, D => 3, E => 4, F => 5, G => 6, H => 7);

pub trait Pass {
    type Binds: Binds;

    fn shader_defs() -> Vec<ShaderDefVal> { vec![] }
}

pub trait Compute {
    const COMPUTE_SHADER_PATH: &'static str;
}

#[derive(Resource, Deref)]
pub struct PipelineCompute<P: Pass> {
    #[deref]
    layouts: <P::Binds as Binds>::Layout,
    id: CachedComputePipelineId,
}

impl<P: Pass + Compute> PipelineCompute<P> {
    pub fn id(&self) -> CachedComputePipelineId { self.id }
}

impl<P: Pass + Compute> FromWorld for PipelineCompute<P> {
    fn from_world(world: &mut World) -> Self {

        let name = type_name::<P>();
        info!("Creating {name} Compute Pass");

        let device = world.resource::<RenderDevice>();
        let layouts = P::Binds::into_layout(device);
        let shader = world.load_asset(P::COMPUTE_SHADER_PATH);
        let entry_point = "compute".into();
        let pipeline_cache = world.resource_mut::<PipelineCache>();
        let id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor { 
            label: Some(name.into()), 
            layout: layouts.clone().into(),
            shader,
            entry_point,
            push_constant_ranges: vec![],
            shader_defs: P::shader_defs(),
            zero_initialize_workgroup_memory: true,
        });
        Self { layouts, id }
    }
}

pub trait Raster {
    const VERTEX_FRAGMENT_SHADER_PATH: &'static str;

    fn multisample() -> MultisampleState { default() }
    fn vertex_buffers() -> Vec<VertexBufferLayout> { vec![] }
    fn depth_stencil() -> Option<DepthStencilState> { None }
    fn fragment_targets() -> Vec<Option<ColorTargetState>> { vec![] }
}

#[derive(Resource, Deref)]
pub struct RasterPipeline<P: Pass> {
    #[deref]
    layouts: <P::Binds as Binds>::Layout,
    id: CachedRenderPipelineId,
}

impl<P: Pass + Raster> RasterPipeline<P> {
    pub fn id(&self) -> CachedRenderPipelineId { self.id }
}

impl<P: Pass + Raster> FromWorld for RasterPipeline<P> {
    fn from_world(world: &mut World) -> Self {
        
        let name = type_name::<P>();
        info!("Creating {name} Raster Pass");
        
        let vertex = get_vertex::<P>(world);
        let fragment = Some(get_fragment::<P>(world));
        let device = world.resource::<RenderDevice>();
        let layouts = P::Binds::into_layout(device);
        let id = world.resource_mut::<PipelineCache>()
            .queue_render_pipeline(RenderPipelineDescriptor {
                label: Some(name.into()),
                layout: layouts.clone().into(),
                vertex,
                primitive: PrimitiveState { 
                    topology: PrimitiveTopology::TriangleStrip, // quads are drawn using only 4 vertices
                    cull_mode: None, // render billboarded quads with only a front-face and no back-face
                    ..default()
                },
                fragment,
                depth_stencil: P::depth_stencil(),
                multisample: P::multisample(),
                push_constant_ranges: vec![],
                zero_initialize_workgroup_memory: true,
            }
        );
        Self { layouts, id }
    }
}

fn get_vertex<P: Pass + Raster>(world: &mut World) -> VertexState {
    let shader = world.load_asset(P::VERTEX_FRAGMENT_SHADER_PATH);
    let entry_point = "vertex".into();
    let shader_defs = P::shader_defs();
    let buffers = P::vertex_buffers(); 
    VertexState { shader, shader_defs, entry_point, buffers }
}

fn get_fragment<P: Pass + Raster>(world: &mut World) -> FragmentState {
    let shader = world.load_asset(P::VERTEX_FRAGMENT_SHADER_PATH);
    let entry_point = "fragment".into();
    let shader_defs = P::shader_defs();
    let targets = P::fragment_targets();
    FragmentState { shader, shader_defs, entry_point, targets }
}










pub type BindingGroupParam<'a> = (
    Res<'a, RenderAssets<GpuImage>>, 
    Res<'a, FallbackImage>, 
    Res<'a, RenderAssets<GpuShaderStorageBuffer>>
);

/// Gets the params required for AsBindGroup for running a custom render pipeline pass.
/// Currently we're not able to get Res<R> from the World, which is the required format
/// so using transmute as a workaround.
/// 
/// TRACK the issue: https://github.com/bevyengine/bevy/issues/16831
pub fn get_binding_group_params(world: &World) -> BindingGroupParam {
    let gpu_images = world.resource_ref::<RenderAssets<GpuImage>>();
    let gpu_images: Res<RenderAssets<GpuImage>> = unsafe { std::mem::transmute(gpu_images) };
    let fallback_image = world.resource_ref::<FallbackImage>();
    let fallback_image: Res<FallbackImage> = unsafe { std::mem::transmute(fallback_image) };
    let gpu_ssbos = world.resource_ref::<RenderAssets<GpuShaderStorageBuffer>>();
    let gpu_ssbos: Res<RenderAssets<GpuShaderStorageBuffer>> = unsafe { std::mem::transmute(gpu_ssbos) };
    (gpu_images, fallback_image, gpu_ssbos)
}

















// uniform utils

#[derive(AsBindGroup, Deref, DerefMut)]
pub struct Uniform<U: ShaderType + WriteInto> {
    #[uniform(0)]
    pub uniform: U,
}

pub trait IntoUniform: Sized + ShaderType + WriteInto {
    fn into_uniform(self) -> Uniform<Self>;
}

impl<U: ShaderType + WriteInto> IntoUniform for U {
    fn into_uniform(self) -> Uniform<U> {
        Uniform { uniform: self }
    }
}

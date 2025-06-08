use std::{marker::*, ops::*};
use bevy::{app::*, asset::*, ecs::component::*, image::*, math::*, prelude::*};
use bevy::render::{extract_component::*, render_resource::*};
use chain_link::*;

use crate::wgputil::ImageViewBuilder;

#[derive(Default)]
pub struct AndExtract;

type AttachParams<'a, T> = (&'a mut Assets<Image>, &'a mut T, UVec2);

#[derive(Default)]
pub struct AttachPlugin<A, E = ()>(PhantomData<(A, E)>);

impl<A> Plugin for AttachPlugin<A, ()>
where 
    A: Component<Mutability = Mutable>,
    for<'a> AttachPlugin::<A, ()>: Cascade<In<'a> = AttachParams<'a, A>>,
{
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, resize_cascade_system::<A>); // TODO optimal (or configurable) schedule?
    }
}

impl<A: ExtractComponent> Plugin for AttachPlugin<A, AndExtract>
where 
    AttachPlugin<A, ()>: Default + Plugin,
{
    fn build(&self, app: &mut App) {
        app.add_plugins(AttachPlugin::<A, ()>::default());
        app.add_plugins(ExtractComponentPlugin::<A>::default());

    }
}

impl<A: Length, E> Length for AttachPlugin<A, E> {
    type Len = A::Len;
}

impl<const N: usize, A: Attach<N>, E> Chain<N> for AttachPlugin<A, E>
where 
    Self: InRange<N, Self::Len>,
{
    type In<'a> = AttachParams<'a, A>;
    type Out<'a> = AttachParams<'a, A>;

    fn chain((images, attach, physical_target_size): Self::In<'_>) -> Self::Out<'_> {
        let new_size = A::compute_size(physical_target_size);
        let handle = &mut attach[N];
        // TODO why is it possible for the default handle to be a valid asset?
        if let Handle::Weak(AssetId::Uuid { uuid: AssetId::<Image>::DEFAULT_UUID }) = handle {
            debug!("Replacing default handle with new image");
            *handle = images.add(A::new_image(new_size));
        } else if let Some(image) = images.get(&*handle) {
            if image.texture_descriptor.size != new_size {
                // TRACK https://github.com/bevyengine/bevy/pull/19462
                if A::COPY_ON_RESIZE {
                    debug!("Copy-on-resize -> {physical_target_size:?}");
                    images.get_mut(handle).unwrap().resize_in_place(new_size);
                } else {
                    debug!("Default resize -> {physical_target_size:?}");
                    images.get_mut(handle).unwrap().texture_descriptor.size = new_size; // TODO breaks for data: Some(..)?
                }
            }
        } else {
            debug!("Edge case: possibly valid handle, but no image found, creating new one");
            *handle = images.add(A::new_image(new_size));
        }
        return (images, attach, physical_target_size)
    }
}

/// System to trigger a chain-link cascade through all of T's Attach<#> impls.
/// Iterates from 0..=N, sequentially resizing each defined Attach<#> type.
fn resize_cascade_system<A>(
    mut query: Query<(&mut A, &Camera)>, 
    mut images: ResMut<Assets<Image>>
) where
    A: Component<Mutability = Mutable>,
    for<'a> AttachPlugin::<A, ()>: Cascade<In<'a> = AttachParams<'a, A>>
{
    for (mut attach, camera) in &mut query {
        camera.physical_target_size()
            .map(|size| AttachPlugin::<A, ()>::cascade((&mut images, &mut attach, size)));
    }
}

pub trait Attach<const N: usize>
where
    Self: InRange<N, <Self as Length>::Len>,
    Self: Component<Mutability = Mutable>,
    Self: Index<usize, Output = Handle<Image>>,
    Self: IndexMut<usize>,
{    

    // defaults
    const LABEL: Option<&'static str> = None;
    const BLEND_STATE: Option<BlendState> = None;
    const COLOR_WRITES: ColorWrites = ColorWrites::ALL;
    const TEXTURE_ASPECT: TextureAspect = TextureAspect::All;
    const COPY_ON_RESIZE: bool = false;

    // required
    const TEXTURE_FORMAT: TextureFormat;
    const TEXTURE_USAGES: TextureUsages;

    fn color_target_state() -> ColorTargetState {
        ColorTargetState {
            format: Self::TEXTURE_FORMAT,
            blend: Self::BLEND_STATE,
            write_mask: Self::COLOR_WRITES,
        }
    }

    fn compute_size(UVec2 { x: width, y: height }: UVec2) -> Extent3d {
        return Extent3d { width, height, depth_or_array_layers: 1 };
    }

    fn new_image(size: Extent3d) -> Image {
        Image {
            data: None,
            texture_descriptor: TextureDescriptor {
                label: Self::LABEL,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: Self::TEXTURE_FORMAT,
                usage: Self::TEXTURE_USAGES,
                view_formats: &[],
            },
            texture_view_descriptor: Some(Self::texture_view(size).descriptor()),
            ..default()
        }
    }

    fn texture_view(size: Extent3d) -> ImageViewBuilder<'static> {
        ImageViewBuilder::<'static>::default()
            .label(Self::LABEL)
            .format(Some(Self::TEXTURE_FORMAT))
            .dimension(Some(match size.depth_or_array_layers {
                0 => panic!("Cannot have 0 `depth_or_array_layers`"),
                1 => TextureViewDimension::D2,
                _ => TextureViewDimension::D2Array,
            }))
            .usage(Some(Self::TEXTURE_USAGES))
            .aspect(Self::TEXTURE_ASPECT)
            .base_mip_level(0)
            .mip_level_count(None)
            .base_array_layer(0)
            .array_layer_count(None)
    }
}
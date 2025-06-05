use std::{marker::*, ops::*};
use bevy::{app::*, asset::*, ecs::component::*, image::*, math::*, prelude::*};
use bevy::render::render_resource::*;
use crate::chain::*;

pub trait Attachment<const N: usize>
where
    Self: Component<Mutability = Mutable>,
    Self: Index<usize, Output = Handle<Image>>,
    Self: IndexMut<usize>,
{
    const LABEL: Option<&'static str> = None;
    const COPY_ON_RESIZE: bool;
    const TEXTURE_FORMAT: TextureFormat;
    const TEXTURE_ASPECT: TextureAspect;
    const TEXTURE_USAGES: TextureUsages;

    fn color_target_state() -> ColorTargetState {
        ColorTargetState {
            format: Self::TEXTURE_FORMAT,
            write_mask: ColorWrites::ALL,
            blend: None,
        }
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
            texture_view_descriptor: Some(Self::texture_view_descriptor()),
            ..default()
        }
    }

    fn texture_view_descriptor() -> TextureViewDescriptor<'static> {
        TextureViewDescriptor {
            label: Self::LABEL,
            format: Some(Self::TEXTURE_FORMAT),
            dimension: Some(TextureViewDimension::D2),
            usage: Some(Self::TEXTURE_USAGES),
            aspect: Self::TEXTURE_ASPECT,
            ..default()
        }
    }
}

type AttachmentParams<'a, T> = (&'a mut Assets<Image>, &'a mut T, Extent3d);

/// System to trigger a chain-link cascade through all of T's Attach<#> impls.
/// Iterates from 0..=N, sequentially resizing each defined Attach<#> type.
fn resize_cascade_system<T>(
    mut query: Query<(&mut T, &Camera)>, 
    mut images: ResMut<Assets<Image>>
) where
    T: Component<Mutability = Mutable>,
    for<'a> T: Cascade<In<'a> = AttachmentParams<'a, T>>
{
    for (mut attach, camera) in &mut query {
        if let Some(UVec2 { x: width, y: height }) = camera.physical_target_size() {
            let size = Extent3d { width, height, depth_or_array_layers: 1};
            T::cascade((&mut images, &mut attach, size));
        }
    }
}

/// Resize functionality per link of the resize chain.
impl<const N: usize, T> Chain<N> for T
where
    T: Attachment<N>,
    T: InRange<N, <T as Length>::Len>,
{
    type In<'a> = AttachmentParams<'a, T>;
    type Out<'a> = AttachmentParams<'a, T>;

    fn chain((images, attach, new_size): Self::In<'_>) -> Self::Out<'_> {
        let handle = &mut attach[N];
        if let Handle::Weak(AssetId::Uuid { uuid: AssetId::<Image>::DEFAULT_UUID }) = handle {
            debug!("Replacing default handle with new image");
            *handle = images.add(T::new_image(new_size));
        } else if let Some(image) = images.get(&*handle) {
            if image.texture_descriptor.size != new_size {
                if T::COPY_ON_RESIZE {
                    debug!("Copy-on-resize -> {new_size:?}");
                    images.get_mut(handle).unwrap().resize_in_place(new_size);
                } else {
                    debug!("Default resize -> {new_size:?}");
                    images.get_mut(handle).unwrap().texture_descriptor.size = new_size; // TODO breaks for data: Some(..)?
                }
            }
        } else {
            debug!("Edge case: possibly valid handle, but no image found, creating new one");
            *handle = images.add(T::new_image(new_size));
        }
        return (images, attach, new_size)
    }
}

#[derive(Default)]
pub struct AttachmentPlugin<T>(PhantomData<T>);

impl<T> Plugin for AttachmentPlugin<T>
where 
    T: Component<Mutability = Mutable>,
    for<'a> T: Cascade<In<'a> = AttachmentParams<'a, T>>,
{
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, resize_cascade_system::<T>); // TODO optimal (or configurable) schedule?
    }
}

/// * possible to turn this into a more general on-resize?
///   or at least support trait-driven on-resize functionality
/// * mip-mapping support + integrated with custom resize logic so we can do things like 
///   scale the texture up to the next power of 2 and mip-map all the way down to 1x1 texel (for hi-z buffer generation)
/// * possible to support 1D + 3D + 2D array textures?
const _TODO: () = ();
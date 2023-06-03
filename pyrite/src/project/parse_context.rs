use std::error::Error;

use mlua::Lua;
use typed_nodes::{TableId, TableIdSource};

use super::{meshes::MeshLoader, spectra::SpectrumLoader, textures::TextureLoader, NodeId, Nodes};

pub(crate) struct ParseContext<'a, 'lua> {
    lua: &'lua Lua,
    nodes: &'a mut Nodes,
    texture_loader: &'a mut TextureLoader,
    mesh_loader: &'a mut MeshLoader,
    spectrum_loader: &'a mut SpectrumLoader,
    id_source: TableIdSource,
}

impl<'a, 'lua> ParseContext<'a, 'lua> {
    pub(crate) fn new(
        lua: &'lua Lua,
        nodes: &'a mut Nodes,
        texture_loader: &'a mut TextureLoader,
        mesh_loader: &'a mut MeshLoader,
        spectrum_loader: &'a mut SpectrumLoader,
    ) -> Self {
        Self {
            lua,
            nodes,
            texture_loader,
            mesh_loader,
            spectrum_loader,
            id_source: TableIdSource::new(),
        }
    }

    pub(crate) fn get_texture_loader(&mut self) -> &mut TextureLoader {
        self.texture_loader
    }

    pub(crate) fn get_mesh_loader(&mut self) -> &mut MeshLoader {
        self.mesh_loader
    }

    pub(crate) fn get_spectrum_loader(&mut self) -> &mut SpectrumLoader {
        self.spectrum_loader
    }
}

impl<'a, 'lua> typed_nodes::Context for ParseContext<'a, 'lua> {
    type NodeId = NodeId;
    type Bounds = typed_nodes::bounds::SendSyncBounds;

    fn get_nodes(&self) -> &Nodes {
        &self.nodes
    }

    fn get_nodes_mut(&mut self) -> &mut Nodes {
        &mut self.nodes
    }
}

impl<'a, 'lua> typed_nodes::FromLuaContext<'lua> for ParseContext<'a, 'lua> {
    type Error = Box<dyn Error>;

    fn get_lua(&self) -> &'lua Lua {
        self.lua
    }

    fn next_table_id(&mut self) -> TableId {
        self.id_source.next_table_id()
    }

    fn table_id_to_node_id(&self, id: TableId) -> Self::NodeId {
        NodeId(id)
    }
}

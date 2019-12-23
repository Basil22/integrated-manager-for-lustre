use crate::{
    components::{font_awesome, paging},
    generated::css_classes::C,
    Route,
};
use iml_wire_types::{
    db::{OstPoolRecord, VolumeNodeRecord},
    warp_drive::{Cache, RecordId},
    Filesystem, Host, Label, Target, TargetConfParam,
};
use seed::{prelude::*, *};
use std::{
    collections::HashMap,
    ops::Deref,
    sync::atomic::{self, AtomicU32},
};

static ID: AtomicU32 = AtomicU32::new(1);

fn increment(id: &AtomicU32) -> u32 {
    id.fetch_add(1, atomic::Ordering::SeqCst)
}

fn sort_by_label(xs: &mut Vec<impl Label>) {
    xs.sort_by(|a, b| natord::compare(a.label(), b.label()));
}

fn get_volume_nodes_by_host_id(
    xs: &HashMap<u32, VolumeNodeRecord>,
    host_id: u32,
) -> Vec<&VolumeNodeRecord> {
    xs.values().filter(|v| v.host_id == host_id).collect()
}

fn get_ost_pools_by_fs_id(
    xs: &HashMap<u32, OstPoolRecord>,
    fs_id: u32,
) -> Vec<&OstPoolRecord> {
    xs.values().filter(|v| v.filesystem_id == fs_id).collect()
}

fn get_targets_by_parent_resource(
    cache: &Cache,
    parent_resource_id: ParentResourceId,
    kind: Kind,
) -> Vec<&Target<TargetConfParam>> {
    match parent_resource_id {
        ParentResourceId::OstPool(x) => get_targets_by_pool_id(&cache, x),
        ParentResourceId::Fs(x) => get_targets_by_fs_id(&cache.target, x, kind),
        _ => vec![],
    }
}

fn get_targets_by_pool_id(
    cache: &Cache,
    ostpool_id: u32,
) -> Vec<&Target<TargetConfParam>> {
    let target_ids: Vec<_> = cache
        .ost_pool_osts
        .values()
        .filter(|x| x.ostpool_id == ostpool_id)
        .map(|x| x.managedost_id)
        .collect();

    cache.target.values().filter(|x| target_ids.contains(&x.id)).collect()
}

fn get_targets_by_fs_id(
    xs: &HashMap<u32, Target<TargetConfParam>>,
    fs_id: u32,
    kind: Kind,
) -> Vec<&Target<TargetConfParam>> {
    xs.values()
        .filter(|x| match kind {
            Kind::Mgt => {
                x.kind == "MGT"
                    && x.filesystems
                        .as_ref()
                        .and_then(|ys| ys.iter().find(|y| y.id == fs_id))
                        .is_some()
            }
            Kind::Mdt => x.kind == "MDT" && x.filesystem_id == Some(fs_id),
            Kind::Ost => x.kind == "OST" && x.filesystem_id == Some(fs_id),
            _ => false,
        })
        .collect()
}

#[derive(PartialEq, Debug, Copy, Clone)]
enum Kind {
    Host,
    Fs,
    Volume,
    Mgt,
    Mdt,
    Ost,
    OstPool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ParentResourceId {
    Host(u32),
    Fs(u32),
    OstPool(u32),
}

impl Deref for ParentResourceId {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        match self {
            ParentResourceId::Host(x)
            | ParentResourceId::Fs(x)
            | ParentResourceId::OstPool(x) => x,
        }
    }
}

/// A collection of resources grouped in relation to a particular parent in the tree.
/// For instance, a `TreeCollection` may consist of OSTs belonging to a particular filesystem.
#[derive(Debug)]
struct TreeCollection {
    id: u32,
    open: bool,
    opens: HashMap<ParentResourceId, bool>,
    paging: paging::Model,
    parent_tree_id: u32,
    kind: Kind,
    parent_resource_id: Option<ParentResourceId>,
}

impl TreeCollection {
    fn new(
        parent_resource_id: Option<ParentResourceId>,
        parent_tree_id: u32,
        total: usize,
        kind: Kind,
    ) -> Self {
        let id = increment(&ID);

        TreeCollection {
            id,
            open: false,
            opens: HashMap::new(),
            parent_resource_id,
            parent_tree_id,
            paging: paging::Model::new(total),
            kind,
        }
    }
}

#[derive(Debug)]
pub struct Model(HashMap<u32, TreeCollection>);

impl Model {
    pub fn new() -> Self {
        let mut model = Model(HashMap::new());

        model.add_tree_collection(0, None, 0, Kind::Host).add_tree_collection(
            0,
            None,
            0,
            Kind::Fs,
        );

        model
    }
    fn insert(&mut self, k: u32, v: TreeCollection) -> &mut Self {
        self.0.insert(k, v);

        self
    }
    fn add_tree_collection(
        &mut self,
        parent_id: u32,
        parent_resource_id: Option<ParentResourceId>,
        total: usize,
        kind: Kind,
    ) -> &mut Self {
        if self
            .find_tree_collection(parent_id, parent_resource_id, kind)
            .is_none()
        {
            let t =
                TreeCollection::new(parent_resource_id, parent_id, total, kind);

            self.insert(t.id, t);
        }

        self
    }
    /// Toggles the open state of a given `TreeCollection` child.
    fn toggle_opens(
        &mut self,
        parent_id: u32,
        parent_resource_id: ParentResourceId,
        is_open: bool,
    ) {
        if let Some(x) = self.get_mut(parent_id) {
            x.opens.insert(parent_resource_id, !is_open);
        }
    }
    fn add_child_tree_collections(
        &mut self,
        cache: &Cache,
        parent_id: u32,
        parent_resource_id: ParentResourceId,
    ) -> Option<()> {
        let x = self.get(&parent_id)?;

        match x.kind {
            Kind::Host => {
                let total = get_volume_nodes_by_host_id(
                    &cache.volume_node,
                    *parent_resource_id,
                )
                .len();

                self.add_tree_collection(
                    parent_id,
                    Some(parent_resource_id),
                    total,
                    Kind::Volume,
                );
            }
            Kind::Fs => {
                self.add_tree_collection(
                    parent_id,
                    Some(parent_resource_id),
                    get_targets_by_fs_id(
                        &cache.target,
                        *parent_resource_id,
                        Kind::Mgt,
                    )
                    .len(),
                    Kind::Mgt,
                )
                .add_tree_collection(
                    parent_id,
                    Some(parent_resource_id),
                    get_targets_by_fs_id(
                        &cache.target,
                        *parent_resource_id,
                        Kind::Mdt,
                    )
                    .len(),
                    Kind::Mdt,
                )
                .add_tree_collection(
                    parent_id,
                    Some(parent_resource_id),
                    get_targets_by_fs_id(
                        &cache.target,
                        *parent_resource_id,
                        Kind::Ost,
                    )
                    .len(),
                    Kind::Ost,
                )
                .add_tree_collection(
                    parent_id,
                    Some(parent_resource_id),
                    get_ost_pools_by_fs_id(
                        &cache.ost_pool,
                        *parent_resource_id,
                    )
                    .len(),
                    Kind::OstPool,
                );
            }
            Kind::OstPool => {
                self.add_tree_collection(
                    parent_id,
                    Some(parent_resource_id),
                    get_targets_by_parent_resource(
                        &cache,
                        parent_resource_id,
                        Kind::Ost,
                    )
                    .len(),
                    Kind::Ost,
                );
            }
            _ => {}
        }

        Some(())
    }
    fn get(&self, k: &u32) -> Option<&TreeCollection> {
        self.0.get(k)
    }
    fn get_mut(&mut self, k: u32) -> Option<&mut TreeCollection> {
        self.0.get_mut(&k)
    }
    fn find_tree_collection_mut(
        &mut self,
        parent_id: u32,
        parent_resource_id: Option<ParentResourceId>,
        kind: Kind,
    ) -> Option<&mut TreeCollection> {
        self.0.values_mut().find(|x| {
            x.kind == kind
                && x.parent_tree_id == parent_id
                && x.parent_resource_id == parent_resource_id
        })
    }
    fn find_tree_collection(
        &self,
        parent_id: u32,
        parent_resource_id: Option<ParentResourceId>,
        kind: Kind,
    ) -> Option<&TreeCollection> {
        self.0.values().find(|x| {
            x.kind == kind
                && x.parent_tree_id == parent_id
                && x.parent_resource_id == parent_resource_id
        })
    }
}

#[derive(Clone)]
pub enum Msg {
    Add(RecordId),
    Remove(RecordId),
    Reset,
    ToggleItem(u32, ParentResourceId, bool),
    ToggleCollection(u32, bool),
    Page(u32, paging::Msg),
}

pub fn update(
    cache: &Cache,
    msg: Msg,
    model: &mut Model,
    _orders: &mut impl Orders<Msg>,
) {
    match msg {
        Msg::Reset => {
            model
                .find_tree_collection_mut(0, None, Kind::Host)
                .map(|x| x.paging.total = cache.host.len());

            model
                .find_tree_collection_mut(0, None, Kind::Fs)
                .map(|x| x.paging.total = cache.filesystem.len());
        }
        Msg::Add(x) => match x {
            RecordId::Host(x) => {
                model
                    .find_tree_collection_mut(0, None, Kind::Host)
                    .map(|x| x.paging.total += 1);
            }
            RecordId::Filesystem(x) => {
                model
                    .find_tree_collection_mut(0, None, Kind::Fs)
                    .map(|x| x.paging.total += 1);
            }
            _ => {}
        },
        Msg::Remove(x) => match x {
            RecordId::Host(x) => {
                model
                    .find_tree_collection_mut(0, None, Kind::Host)
                    .map(|x| x.paging.total -= 1);
            }
            RecordId::Filesystem(x) => {
                model
                    .find_tree_collection_mut(0, None, Kind::Fs)
                    .map(|x| x.paging.total -= 1);
            }
            _ => {}
        },
        Msg::ToggleItem(parent_id, parent_resource_id, open) => {
            model.toggle_opens(parent_id, parent_resource_id, open);
            model.add_child_tree_collections(
                cache,
                parent_id,
                parent_resource_id,
            );
        }
        Msg::ToggleCollection(id, open) => {
            model.get_mut(id).map(|mut x| x.open = !open);
        }
        Msg::Page(id, msg) => {
            model.get_mut(id).map(|x| paging::update(msg, &mut x.paging));
        }
    }
}

fn pager_view(paging: &paging::Model) -> Node<paging::Msg> {
    if !paging.has_pages() {
        return empty!();
    }

    li![
        class![C.py_1],
        a![
            class![
                C.px_5, 
                C.hover__underline,
                C.select_none,
                C.hover__text_gray_300,
                C.cursor_pointer,
                C.pointer_events_none => !paging.has_less()],
            font_awesome(
                class![C.w_5, C.h_4, C.inline, C.mr_1],
                "chevron-left",
            ),
            simple_ev(Ev::Click, paging::Msg::Prev),
            "prev"
        ],
        a![
            class![
                C.hover__underline,
                C.select_none,
                C.hover__text_gray_300,
                C.cursor_pointer,
                C.pointer_events_none => !paging.has_more()],
            "next",
            simple_ev(Ev::Click, paging::Msg::Next),
            font_awesome(
                class![C.w_5, C.h_4, C.inline, C.mr_1],
                "chevron-right",
            )
        ]
    ]
}

fn toggle_view(msg: Msg, is_open: bool) -> Node<Msg> {
    let mut toggle = font_awesome(
        class![C.w_5, C.h_4 C.inline, C.mr_1, C.cursor_pointer],
        "chevron-right",
    );

    toggle.add_listener(mouse_ev(Ev::Click, move |_| msg));

    if is_open {
        toggle.add_style(St::Transform, "rotate(90deg)");
    }

    toggle
}

fn item_view(icon: &str, label: &str, route: Route) -> Node<Msg> {
    a![
        class![C.hover__underline, C.hover__text_gray_300],
        attrs! {
            At::Href => route.to_href()
        },
        font_awesome(class![C.w_5, C.h_4, C.inline, C.mr_1], icon),
        label
    ]
}

fn tree_host_item_view(
    cache: &Cache,
    model: &Model,
    parent: &TreeCollection,
    host: &Host,
) -> Node<Msg> {
    let parent_resource_id = ParentResourceId::Host(host.id);

    let open = *parent.opens.get(&parent_resource_id).unwrap_or(&false);

    li![
        class![C.py_1],
        toggle_view(Msg::ToggleItem(parent.id, parent_resource_id, open), open),
        item_view("server", &host.label, Route::ServerDetail(host.id.into())),
        if open {
            tree_volume_collection_view(
                cache,
                model,
                parent.id,
                parent_resource_id,
            )
        } else {
            empty!()
        }
    ]
}

fn tree_pool_item_view(
    cache: &Cache,
    model: &Model,
    parent: &TreeCollection,
    pool: &OstPoolRecord,
) -> Node<Msg> {
    let parent_resource_id = ParentResourceId::OstPool(pool.id);
    let open = *parent.opens.get(&parent_resource_id).unwrap_or(&false);

    li![
        class![C.py_1],
        toggle_view(Msg::ToggleItem(parent.id, parent_resource_id, open), open),
        item_view("swimming-pool", &pool.label(), Route::Target),
        if open {
            tree_target_collection_view(
                cache,
                model,
                Kind::Ost,
                parent.id,
                parent_resource_id,
            )
        } else {
            empty!()
        }
    ]
}

fn tree_fs_item_view(
    cache: &Cache,
    model: &Model,
    parent: &TreeCollection,
    fs: &Filesystem,
) -> Node<Msg> {
    let parent_resource_id = ParentResourceId::Fs(fs.id);

    let open = *parent.opens.get(&parent_resource_id).unwrap_or(&false);

    li![
        class![C.py_1],
        toggle_view(Msg::ToggleItem(parent.id, parent_resource_id, open), open),
        item_view("server", &fs.label, Route::FilesystemDetail(fs.id.into())),
        if open {
            vec![
                tree_target_collection_view(
                    cache,
                    model,
                    Kind::Mgt,
                    parent.id,
                    parent_resource_id,
                ),
                tree_target_collection_view(
                    cache,
                    model,
                    Kind::Mdt,
                    parent.id,
                    parent_resource_id,
                ),
                tree_target_collection_view(
                    cache,
                    model,
                    Kind::Ost,
                    parent.id,
                    parent_resource_id,
                ),
                tree_pools_collection_view(
                    cache,
                    model,
                    parent.id,
                    parent_resource_id,
                ),
            ]
        } else {
            vec![]
        }
    ]
}

fn tree_collection_view(
    model: &Model,
    parent_id: u32,
    parent_resource_id: Option<ParentResourceId>,
    kind: Kind,
    item: impl FnOnce(&TreeCollection) -> Node<Msg>,
    on_open: impl FnOnce(&TreeCollection) -> Node<Msg>,
) -> Option<Node<Msg>> {
    let x = model.find_tree_collection(parent_id, parent_resource_id, kind)?;

    let id = x.id;

    let el = ul![
        class![C.px_6, C.mt_2],
        toggle_view(Msg::ToggleCollection(id, x.open,), x.open,),
        item(&x),
        if x.open {
            on_open(&x)
        } else {
            empty![]
        }
    ];

    Some(el)
}

fn tree_fs_collection_view(cache: &Cache, model: &Model) -> Node<Msg> {
    tree_collection_view(
        model,
        0,
        None,
        Kind::Fs,
        |x| {
            item_view(
                "folder",
                &format!("Filesystems ({})", x.paging.total),
                Route::Filesystem,
            )
        },
        |x| {
            let mut xs: Vec<_> = cache.filesystem.values().collect();

            sort_by_label(&mut xs);

            ul![
                class![C.px_6, C.mt_2],
                xs.into_iter()
                    .map(|y| { tree_fs_item_view(&cache, &model, &x, y) })
            ]
        },
    )
    .unwrap_or(empty![])
}

fn tree_host_collection_view(cache: &Cache, model: &Model) -> Node<Msg> {
    tree_collection_view(
        model,
        0,
        None,
        Kind::Host,
        |x| {
            item_view(
                "folder",
                &format!("Servers ({})", x.paging.total),
                Route::Server,
            )
        },
        |x| {
            let mut hosts: Vec<_> = cache.host.values().collect();

            sort_by_label(&mut hosts);

            ul![
                class![C.px_6, C.mt_2],
                hosts
                    .into_iter()
                    .map(|y| { tree_host_item_view(&cache, &model, x, y) })
            ]
        },
    )
    .unwrap_or(empty![])
}

fn tree_pools_collection_view(
    cache: &Cache,
    model: &Model,
    parent_id: u32,
    parent_resource_id: ParentResourceId,
) -> Node<Msg> {
    tree_collection_view(
        model,
        parent_id,
        Some(parent_resource_id),
        Kind::OstPool,
        |x| {
            item_view(
                "folder",
                &format!("OST Pools ({})", x.paging.total),
                Route::Target,
            )
        },
        |x| {
            let id = x.id;

            let mut xs: Vec<_> =
                get_ost_pools_by_fs_id(&cache.ost_pool, *parent_resource_id);

            sort_by_label(&mut xs);

            ul![
                class![C.px_6, C.mt_2],
                paging::slice_page(&xs, &x.paging)
                    .iter()
                    .map(|y| { tree_pool_item_view(cache, model, x, y) }),
                pager_view(&x.paging)
                    .map_message(move |msg| { Msg::Page(id, msg) })
            ]
        },
    )
    .unwrap_or(empty![])
}

fn tree_volume_collection_view(
    cache: &Cache,
    model: &Model,
    parent_id: u32,
    parent_resource_id: ParentResourceId,
) -> Node<Msg> {
    tree_collection_view(
        model,
        parent_id,
        Some(parent_resource_id),
        Kind::Volume,
        |x| {
            item_view(
                "folder",
                &format!("Volumes ({})", x.paging.total),
                Route::Volume,
            )
        },
        |x| {
            let id = x.id;

            let mut volume_nodes: Vec<_> = get_volume_nodes_by_host_id(
                &cache.volume_node,
                *parent_resource_id,
            );

            sort_by_label(&mut volume_nodes);

            ul![
                class![C.px_6, C.mt_2],
                paging::slice_page(&volume_nodes, &x.paging).iter().map(|x| {
                    let v = cache
                        .volume
                        .values()
                        .find(|v| v.id == x.volume_id)
                        .unwrap();

                    li![
                        class![C.py_1],
                        item_view("hdd", &x.label(), Route::Volume),
                    ]
                }),
                pager_view(&x.paging)
                    .map_message(move |msg| { Msg::Page(id, msg) })
            ]
        },
    )
    .unwrap_or(empty![])
}

fn tree_target_collection_view(
    cache: &Cache,
    model: &Model,
    kind: Kind,
    parent_id: u32,
    parent_resource_id: ParentResourceId,
) -> Node<Msg> {
    let label = match kind {
        Kind::Mgt => "MGTs",
        Kind::Mdt => "MDTs",
        Kind::Ost => "OSTs",
        _ => "",
    };

    tree_collection_view(
        model,
        parent_id,
        Some(parent_resource_id),
        kind,
        |x| {
            item_view(
                "folder",
                &format!("{} ({})", label, x.paging.total),
                Route::Target,
            )
        },
        |x| {
            let id = x.id;

            let mut xs: Vec<_> = get_targets_by_parent_resource(
                &cache,
                parent_resource_id,
                kind,
            );

            sort_by_label(&mut xs);

            ul![
                class![C.px_6, C.mt_2],
                paging::slice_page(&xs, &x.paging).iter().map(|x| {
                    li![
                        class![C.py_1],
                        item_view("bullseye", &x.label(), Route::Target),
                    ]
                }),
                pager_view(&x.paging)
                    .map_message(move |msg| { Msg::Page(id, msg) })
            ]
        },
    )
    .unwrap_or(empty![])
}

pub fn view(cache: &Cache, model: &Model) -> Node<Msg> {
    div![
        class![C.p_5, C.text_gray_500],
        tree_host_collection_view(cache, model),
        tree_fs_collection_view(cache, model)
    ]
}

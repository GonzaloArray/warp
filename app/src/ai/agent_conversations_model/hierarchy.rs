use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};
use warpui::{AppContext, SingletonEntity};

use super::{
    AgentConversationEntry, AgentConversationEntryId, AgentConversationsModel,
    AgentManagementFilters, AgentRunDisplayStatus, OwnerFilter, entry,
};
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::ai::blocklist::BlocklistAIHistoryModel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentHierarchyAvailability {
    Available,
    Unavailable,
    Detached,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentHierarchyAttention {
    Blocked,
    Failed,
    Working,
    Done,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AgentHierarchyCounts {
    pub blocked: usize,
    pub failed: usize,
    pub working: usize,
    pub done: usize,
    pub unavailable: usize,
}

impl AgentHierarchyCounts {
    pub fn attention(self) -> Option<AgentHierarchyAttention> {
        if self.blocked > 0 {
            Some(AgentHierarchyAttention::Blocked)
        } else if self.failed > 0 {
            Some(AgentHierarchyAttention::Failed)
        } else if self.working > 0 {
            Some(AgentHierarchyAttention::Working)
        } else if self.done > 0 {
            Some(AgentHierarchyAttention::Done)
        } else if self.unavailable > 0 {
            Some(AgentHierarchyAttention::Unavailable)
        } else {
            None
        }
    }

    fn add_status(&mut self, status: HierarchyStatus) {
        match status {
            HierarchyStatus::Blocked => self.blocked += 1,
            HierarchyStatus::Failed => self.failed += 1,
            HierarchyStatus::Working => self.working += 1,
            HierarchyStatus::Done => self.done += 1,
            HierarchyStatus::Unavailable => self.unavailable += 1,
        }
    }

    fn add_counts(&mut self, other: Self) {
        self.blocked += other.blocked;
        self.failed += other.failed;
        self.working += other.working;
        self.done += other.done;
        self.unavailable += other.unavailable;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentHierarchyNode {
    pub id: AgentConversationEntryId,
    pub parent_id: Option<AgentConversationEntryId>,
    pub depth: usize,
    pub entry: Option<AgentConversationEntry>,
    pub availability: AgentHierarchyAvailability,
    pub has_children: bool,
    pub descendants: AgentHierarchyCounts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HierarchyStatus {
    Blocked,
    Failed,
    Working,
    Done,
    Unavailable,
}

impl HierarchyStatus {
    fn from_display(status: &AgentRunDisplayStatus) -> Self {
        match status {
            AgentRunDisplayStatus::TaskBlocked { .. }
            | AgentRunDisplayStatus::ConversationBlocked { .. } => Self::Blocked,
            AgentRunDisplayStatus::TaskFailed
            | AgentRunDisplayStatus::TaskError
            | AgentRunDisplayStatus::TaskCancelled
            | AgentRunDisplayStatus::ConversationError
            | AgentRunDisplayStatus::ConversationCancelled => Self::Failed,
            AgentRunDisplayStatus::TaskQueued
            | AgentRunDisplayStatus::TaskPending
            | AgentRunDisplayStatus::TaskClaimed
            | AgentRunDisplayStatus::TaskInProgress
            | AgentRunDisplayStatus::ConversationInProgress => Self::Working,
            AgentRunDisplayStatus::TaskSucceeded | AgentRunDisplayStatus::ConversationSucceeded => {
                Self::Done
            }
            AgentRunDisplayStatus::TaskUnknown => Self::Unavailable,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct HierarchySource {
    pub id: AgentConversationEntryId,
    pub parent_id: Option<AgentConversationEntryId>,
    pub parent_declared: bool,
    pub invalid_parent: bool,
    pub conflicting_parent: bool,
    pub explicit_children: Vec<AgentConversationEntryId>,
    pub created_at: Option<DateTime<Utc>>,
    pub status: Option<HierarchyStatus>,
    pub matches_filters: bool,
    pub available: bool,
}

impl HierarchySource {
    fn unavailable(id: AgentConversationEntryId, parent_id: AgentConversationEntryId) -> Self {
        Self {
            id,
            parent_id: Some(parent_id),
            parent_declared: false,
            invalid_parent: false,
            conflicting_parent: false,
            explicit_children: Vec::new(),
            created_at: None,
            status: None,
            matches_filters: false,
            available: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Validity {
    Valid,
    Detached,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectedHierarchyNode {
    pub id: AgentConversationEntryId,
    pub parent_id: Option<AgentConversationEntryId>,
    pub depth: usize,
    pub availability: AgentHierarchyAvailability,
    pub has_children: bool,
    pub descendants: AgentHierarchyCounts,
}

pub(super) fn project_topology(
    sources: Vec<HierarchySource>,
    retain_all: bool,
) -> Vec<ProjectedHierarchyNode> {
    let mut sources = sources
        .into_iter()
        .map(|source| (source.id, source))
        .collect::<HashMap<_, _>>();
    attach_explicit_children(&mut sources);

    let validity = validate_sources(&sources);
    let children_by_parent = ordered_children_by_parent(&sources);
    let retained = retained_ids(&sources, &children_by_parent, retain_all);

    let mut roots = retained
        .iter()
        .copied()
        .filter(|id| {
            validity.get(id) == Some(&Validity::Valid)
                && sources
                    .get(id)
                    .is_some_and(|source| source.parent_id.is_none())
        })
        .collect::<Vec<_>>();
    sort_ids(&mut roots, &sources);

    let mut projected = Vec::new();
    let mut visited = HashSet::new();
    for root in roots {
        visit_from(
            root,
            &sources,
            &validity,
            &children_by_parent,
            &retained,
            &mut projected,
            &mut visited,
        );
    }

    let mut detached_tail = retained
        .iter()
        .copied()
        .filter(|id| !visited.contains(id))
        .collect::<Vec<_>>();
    sort_ids(&mut detached_tail, &sources);
    for id in detached_tail {
        if !visited.contains(&id) {
            visit_from(
                id,
                &sources,
                &validity,
                &children_by_parent,
                &retained,
                &mut projected,
                &mut visited,
            );
        }
    }
    projected
}

fn attach_explicit_children(sources: &mut HashMap<AgentConversationEntryId, HierarchySource>) {
    let mut parent_ids = sources.keys().copied().collect::<Vec<_>>();
    sort_ids(&mut parent_ids, sources);

    let mut parent_claims =
        HashMap::<AgentConversationEntryId, Vec<AgentConversationEntryId>>::new();
    for parent_id in parent_ids {
        for child_id in sources
            .get(&parent_id)
            .into_iter()
            .flat_map(|source| &source.explicit_children)
        {
            let claims = parent_claims.entry(*child_id).or_default();
            if !claims.contains(&parent_id) {
                claims.push(parent_id);
            }
        }
    }

    for (child_id, claims) in parent_claims {
        let first_parent = claims[0];
        sources
            .entry(child_id)
            .or_insert_with(|| HierarchySource::unavailable(child_id, first_parent));
        let child = sources
            .get_mut(&child_id)
            .expect("explicit child source was inserted above");
        if claims.len() > 1 && !child.parent_declared {
            child.parent_id = None;
            child.conflicting_parent = true;
            continue;
        }
        match child.parent_id {
            Some(existing_parent) if claims.iter().any(|parent| *parent != existing_parent) => {
                child.conflicting_parent = true;
            }
            Some(_) => {}
            None if child.parent_declared => child.conflicting_parent = true,
            None => child.parent_id = Some(first_parent),
        }
    }
}

fn validate_sources(
    sources: &HashMap<AgentConversationEntryId, HierarchySource>,
) -> HashMap<AgentConversationEntryId, Validity> {
    let mut validity = HashMap::new();
    let mut ids = sources.keys().copied().collect::<Vec<_>>();
    sort_ids(&mut ids, sources);

    for start in ids {
        if validity.contains_key(&start) {
            continue;
        }
        let mut path = Vec::new();
        let mut positions = HashSet::new();
        let mut current = start;
        let result = loop {
            if let Some(result) = validity.get(&current) {
                break *result;
            }
            if !positions.insert(current) {
                break Validity::Detached;
            }
            let Some(source) = sources.get(&current) else {
                break Validity::Detached;
            };
            path.push(current);
            if source.invalid_parent || source.conflicting_parent {
                break Validity::Detached;
            }
            match source.parent_id {
                Some(parent_id) if sources.contains_key(&parent_id) => current = parent_id,
                Some(_) => break Validity::Detached,
                None => break Validity::Valid,
            }
        };
        for id in path {
            validity.insert(id, result);
        }
    }
    validity
}

fn ordered_children_by_parent(
    sources: &HashMap<AgentConversationEntryId, HierarchySource>,
) -> HashMap<AgentConversationEntryId, Vec<AgentConversationEntryId>> {
    let mut children_by_parent = HashMap::new();
    for (parent_id, source) in sources {
        let mut children = Vec::new();
        for child_id in &source.explicit_children {
            if sources
                .get(child_id)
                .is_some_and(|child| child.parent_id == Some(*parent_id))
                && !children.contains(child_id)
            {
                children.push(*child_id);
            }
        }
        let mut fallback = sources
            .values()
            .filter(|child| child.parent_id == Some(*parent_id) && !children.contains(&child.id))
            .map(|child| child.id)
            .collect::<Vec<_>>();
        sort_ids(&mut fallback, sources);
        children.extend(fallback);
        children_by_parent.insert(*parent_id, children);
    }
    children_by_parent
}

fn retained_ids(
    sources: &HashMap<AgentConversationEntryId, HierarchySource>,
    children_by_parent: &HashMap<AgentConversationEntryId, Vec<AgentConversationEntryId>>,
    retain_all: bool,
) -> HashSet<AgentConversationEntryId> {
    let mut retained = sources
        .values()
        .filter(|source| retain_all || source.matches_filters)
        .map(|source| source.id)
        .collect::<HashSet<_>>();
    let matches = retained.iter().copied().collect::<Vec<_>>();
    for id in matches {
        let mut current = sources.get(&id).and_then(|source| source.parent_id);
        let mut visited = HashSet::new();
        while let Some(parent_id) = current {
            if !visited.insert(parent_id) {
                break;
            }
            let Some(parent) = sources.get(&parent_id) else {
                break;
            };
            retained.insert(parent_id);
            current = parent.parent_id;
        }
    }

    let mut retained_parents = retained.iter().copied().collect::<VecDeque<_>>();
    while let Some(parent_id) = retained_parents.pop_front() {
        for child_id in children_by_parent.get(&parent_id).into_iter().flatten() {
            if sources
                .get(child_id)
                .is_some_and(|source| !source.available)
                && retained.insert(*child_id)
            {
                retained_parents.push_back(*child_id);
            }
        }
    }
    retained
}

fn visit_from(
    root: AgentConversationEntryId,
    sources: &HashMap<AgentConversationEntryId, HierarchySource>,
    validity: &HashMap<AgentConversationEntryId, Validity>,
    children_by_parent: &HashMap<AgentConversationEntryId, Vec<AgentConversationEntryId>>,
    retained: &HashSet<AgentConversationEntryId>,
    projected: &mut Vec<ProjectedHierarchyNode>,
    visited: &mut HashSet<AgentConversationEntryId>,
) {
    enum Frame {
        Enter {
            id: AgentConversationEntryId,
            depth: usize,
            render_parent: Option<AgentConversationEntryId>,
        },
        Exit {
            id: AgentConversationEntryId,
            render_parent: Option<AgentConversationEntryId>,
        },
    }

    let mut stack = vec![Frame::Enter {
        id: root,
        depth: 0,
        render_parent: None,
    }];
    let mut active = HashSet::new();
    let mut indices = HashMap::new();

    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Enter {
                id,
                depth,
                render_parent,
            } => {
                if active.contains(&id) || !retained.contains(&id) || !visited.insert(id) {
                    continue;
                }
                let Some(source) = sources.get(&id) else {
                    continue;
                };
                active.insert(id);
                let children = children_by_parent
                    .get(&id)
                    .into_iter()
                    .flatten()
                    .filter(|child_id| retained.contains(child_id))
                    .copied()
                    .collect::<Vec<_>>();
                let availability = match validity.get(&id) {
                    Some(Validity::Detached) | None => AgentHierarchyAvailability::Detached,
                    Some(Validity::Valid) if source.available => {
                        AgentHierarchyAvailability::Available
                    }
                    Some(Validity::Valid) => AgentHierarchyAvailability::Unavailable,
                };
                indices.insert(id, projected.len());
                projected.push(ProjectedHierarchyNode {
                    id,
                    parent_id: source.parent_id,
                    depth,
                    availability,
                    has_children: !children.is_empty(),
                    descendants: AgentHierarchyCounts::default(),
                });
                stack.push(Frame::Exit { id, render_parent });
                for child_id in children.into_iter().rev() {
                    stack.push(Frame::Enter {
                        id: child_id,
                        depth: depth + 1,
                        render_parent: Some(id),
                    });
                }
            }
            Frame::Exit { id, render_parent } => {
                active.remove(&id);
                let Some(index) = indices.get(&id).copied() else {
                    continue;
                };
                let mut subtree = projected[index].descendants;
                let source = sources.get(&id).expect("projected source must exist");
                subtree.add_status(source.status.unwrap_or(HierarchyStatus::Unavailable));
                if let Some(parent_id) = render_parent
                    && let Some(parent_index) = indices.get(&parent_id).copied()
                {
                    projected[parent_index].descendants.add_counts(subtree);
                }
            }
        }
    }
}

fn sort_ids(
    ids: &mut [AgentConversationEntryId],
    sources: &HashMap<AgentConversationEntryId, HierarchySource>,
) {
    ids.sort_by(|a, b| {
        let a_source = sources.get(a);
        let b_source = sources.get(b);
        a_source
            .and_then(|source| source.created_at)
            .cmp(&b_source.and_then(|source| source.created_at))
            .then_with(|| a.as_key().cmp(&b.as_key()))
    });
}

pub(super) fn get_hierarchy(
    model: &AgentConversationsModel,
    filters: &AgentManagementFilters,
    app: &AppContext,
) -> Vec<AgentHierarchyNode> {
    let history = BlocklistAIHistoryModel::as_ref(app);
    let mut sources = HashMap::new();
    let mut entries = HashMap::new();
    let mut task_by_conversation = HashMap::new();

    for task in model.tasks.values() {
        if let Some(conversation_id) = entry::conversation_id_shadowed_by_task(task, history) {
            insert_task_conversation_mapping(
                &mut task_by_conversation,
                conversation_id,
                task.task_id,
            );
        }
        let normalized = entry::entry_for_task(task, history, app);
        let id = normalized.id;
        let (parent_id, invalid_parent) = match task.parent_run_id.as_deref() {
            Some(raw_parent) => match raw_parent.parse() {
                Ok(parent_task_id) => (
                    Some(AgentConversationEntryId::AmbientRun(parent_task_id)),
                    false,
                ),
                Err(_) => (None, true),
            },
            None => (None, false),
        };
        let explicit_children = task
            .children
            .iter()
            .filter_map(|child_id| child_id.parse().ok())
            .map(AgentConversationEntryId::AmbientRun)
            .collect();
        sources.insert(
            id,
            HierarchySource {
                id,
                parent_id,
                parent_declared: task.parent_run_id.is_some(),
                invalid_parent,
                conflicting_parent: false,
                explicit_children,
                created_at: Some(normalized.display.created_at),
                status: Some(HierarchyStatus::from_display(&normalized.display.status)),
                matches_filters: normalized.matches_filters(filters, app),
                available: true,
            },
        );
        entries.insert(id, normalized);
    }

    let mut conversation_queue = model.conversations.keys().copied().collect::<VecDeque<_>>();
    conversation_queue.extend(history.all_known_conversation_ids());
    conversation_queue.extend(task_by_conversation.keys().copied());
    let mut scanned = HashSet::new();

    while let Some(conversation_id) = conversation_queue.pop_front() {
        if !scanned.insert(conversation_id) {
            continue;
        }
        let parent_id = canonical_conversation_id(conversation_id, &task_by_conversation);
        ensure_conversation_source(parent_id, model, filters, app, &mut sources, &mut entries);
        for child_conversation_id in history.child_conversation_ids_of(&conversation_id) {
            let child_id = canonical_conversation_id(*child_conversation_id, &task_by_conversation);
            ensure_conversation_source(child_id, model, filters, app, &mut sources, &mut entries);
            if let Some(parent) = sources.get_mut(&parent_id) {
                parent.explicit_children.push(child_id);
            }
            conversation_queue.push_back(*child_conversation_id);
        }
    }

    let all_owner_filters = AgentManagementFilters {
        owners: OwnerFilter::All,
        ..Default::default()
    };
    let projected = project_topology(
        sources.into_values().collect(),
        filters == &all_owner_filters,
    );
    projected
        .into_iter()
        .map(|node| AgentHierarchyNode {
            id: node.id,
            parent_id: node.parent_id,
            depth: node.depth,
            entry: entries.remove(&node.id),
            availability: node.availability,
            has_children: node.has_children,
            descendants: node.descendants,
        })
        .collect()
}

pub(super) fn insert_task_conversation_mapping(
    task_by_conversation: &mut HashMap<AIConversationId, AmbientAgentTaskId>,
    conversation_id: AIConversationId,
    task_id: AmbientAgentTaskId,
) {
    task_by_conversation
        .entry(conversation_id)
        .and_modify(|current| {
            if task_id.to_string() < current.to_string() {
                *current = task_id;
            }
        })
        .or_insert(task_id);
}

fn canonical_conversation_id(
    conversation_id: AIConversationId,
    task_by_conversation: &HashMap<AIConversationId, crate::ai::ambient_agents::AmbientAgentTaskId>,
) -> AgentConversationEntryId {
    task_by_conversation
        .get(&conversation_id)
        .copied()
        .map(AgentConversationEntryId::AmbientRun)
        .unwrap_or(AgentConversationEntryId::Conversation(conversation_id))
}

fn ensure_conversation_source(
    id: AgentConversationEntryId,
    model: &AgentConversationsModel,
    filters: &AgentManagementFilters,
    app: &AppContext,
    sources: &mut HashMap<AgentConversationEntryId, HierarchySource>,
    entries: &mut HashMap<AgentConversationEntryId, AgentConversationEntry>,
) {
    if sources.contains_key(&id) {
        return;
    }
    let entry = model.get_entry_by_id(&id, app);
    let source = HierarchySource {
        id,
        parent_id: None,
        parent_declared: false,
        invalid_parent: false,
        conflicting_parent: false,
        explicit_children: Vec::new(),
        // Conversation metadata only exposes mutable `last_updated`; IDs are the
        // stable structural fallback until a true creation timestamp exists.
        created_at: None,
        status: entry
            .as_ref()
            .map(|entry| HierarchyStatus::from_display(&entry.display.status)),
        matches_filters: entry
            .as_ref()
            .is_some_and(|entry| entry.matches_filters(filters, app)),
        available: entry.is_some(),
    };
    if let Some(entry) = entry {
        entries.insert(id, entry);
    }
    sources.insert(id, source);
}

// DTOs mirroring crates/lull-core (serde camelCase). Keep in sync with model.rs.

export type BoardKind = 'kanban' | 'files';
export type Orientation = 'columns' | 'rows';
export type Spacing = 'fixedMain' | 'fixedCross';

export type Parent = { kind: 'lane'; id: string } | { kind: 'card'; id: string } | { kind: 'root' };

export interface ViewSettings {
  orientation: Orientation;
  spacing: Spacing;
}

export type CoverMode = 'cover' | 'thumb';
export interface Cover {
  file: string;
  mode: CoverMode;
}

export type AttachmentKind = 'image' | 'pdf' | 'video' | 'audio' | 'text' | 'other';
export interface Attachment {
  file: string;
  display: string;
  kind: AttachmentKind;
  size: number;
}

export interface LinkRef {
  id: string | null;
  target: string;
  heading: string | null;
  alias: string | null;
  embed: boolean;
  line: number;
}

export interface Heading {
  level: number;
  text: string;
  line: number;
}

export interface Footer {
  startLine: number | null;
  fields: [string, string][];
  priority: string | null;
  due: string | null;
  start: string | null;
  assignees: string[];
  labels: string[];
}

export interface FaceItem {
  text: string;
  task: boolean | null;
  line: number;
}

export type Face =
  | { kind: 'none' }
  | { kind: 'checklist'; items: FaceItem[]; total: number }
  | { kind: 'list'; items: FaceItem[]; total: number; ordered: boolean }
  | { kind: 'table'; header: string[]; rows: string[][]; total: number }
  | { kind: 'summary'; text: string };

export interface TaskStats {
  total: number;
  done: number;
}

export interface NodeDto {
  id: string;
  parent: Parent;
  isGroup: boolean;
  children: string[];
  archived: boolean;
  cover: Cover | null;
  title: string;
  hasTitleLine: boolean;
  tags: string[];
  links: LinkRef[];
  mentions: string[];
  dates: string[];
  footer: Footer;
  tasks: TaskStats;
  face: Face;
  headings: Heading[];
  attachments: Attachment[];
  mtime: number;
  wordCount: number;
  hasCode: boolean;
  /** Present for remote-linked cards (Jira, Trello…), filled by integrations. */
  remote?: RemoteInfo | null;
}

export interface LaneDto {
  id: string;
  name: string;
  order: string[];
  color: string | null;
  width: number | null;
  wip: number | null;
  collapsed: boolean;
  archived: boolean;
}

export interface BoardHeader {
  id: string;
  name: string;
  kind: BoardKind;
  root: string;
  view: ViewSettings;
  tagColors: Record<string, string>;
  readOnly: string | null;
  warnings: string[];
  schema: number;
}

export interface BoardSnapshot {
  header: BoardHeader;
  lanes: LaneDto[];
  rootOrder: string[];
  nodes: NodeDto[];
  version: number;
}

export interface BoardDelta {
  boardId: string;
  version: number;
  header?: BoardHeader;
  lanes?: LaneDto[];
  rootOrder?: string[];
  nodes: NodeDto[];
  removed: string[];
}

export interface Placement {
  id: string;
  parent: Parent;
  index: number;
}

export interface LanePatch {
  name?: string;
  color?: string;
  width?: number;
  wip?: number;
  collapsed?: boolean;
}

export interface BoardPatch {
  name?: string;
  view?: ViewSettings;
  tagColors?: Record<string, string>;
}

export type Op =
  | { op: 'createCard'; id: string; parent: Parent; index: number | null; content: string }
  | { op: 'writeCard'; id: string; content: string }
  | { op: 'move'; ids: string[]; to: Parent; before: string | null }
  | { op: 'place'; items: Placement[] }
  | { op: 'trash'; nodes: string[]; lanes: string[] }
  | { op: 'restore'; entries: string[] }
  | { op: 'setArchived'; nodes: [string, boolean][]; lanes: [string, boolean][] }
  | { op: 'createLane'; id: string; name: string; index: number | null }
  | { op: 'updateLane'; id: string; patch: LanePatch }
  | { op: 'moveLane'; id: string; index: number }
  | { op: 'updateBoard'; patch: BoardPatch }
  | { op: 'setCover'; id: string; cover: Cover | null }
  | { op: 'setKind'; kind: BoardKind }
  | { op: 'batch'; ops: Op[] };

export interface ApplyResult {
  version: number;
  created: string[];
  trashed: string[];
}

export interface UndoResult {
  label: string | null;
  done: boolean;
}

export interface BoardEntry {
  id: string;
  path: string;
  name: string;
  kind: BoardKind;
  pinned: boolean;
  hidden: boolean;
  missing: boolean;
  mirror: boolean;
  section?: string | null;
  lastOpened?: number | null;
  lastSeen: number;
  duplicates?: string[];
}

export interface Registry {
  boards: BoardEntry[];
  order: string[];
  mirrorOrder: string[];
}

export type Origin = 'you' | 'external' | 'remote';

export interface JournalEntry {
  ts: string;
  board: string;
  kind: string;
  origin: Origin;
  label: string;
  ids: string[];
  details?: unknown;
  before?: string | null;
  after?: string | null;
}

export interface HistoryFilter {
  from?: string;
  to?: string;
  ids?: string[];
  kinds?: string[];
  origins?: Origin[];
  limit?: number;
}

export interface TrashEntry {
  id: string;
  kind: 'node' | 'lane' | 'orphan';
  itemId: string;
  title: string;
  parent: Parent | null;
  index: number;
  isGroup: boolean;
  count: number;
  deletedAt: string;
  boardId: string;
}

export interface SearchHit {
  board: string;
  boardName: string;
  id: string;
  title: string;
  snippet: string;
  laneName: string | null;
  tags: string[];
  kind: string;
  isGroup: boolean;
  archived: boolean;
  mtime: number;
  remoteKey: string | null;
  status: string | null;
}

export interface SearchOptions {
  boards?: string[];
  limit?: number;
  includeArchived?: boolean;
}

export interface AppInfo {
  name: string;
  version: string;
  platform: 'macos' | 'windows' | 'linux' | string;
  arch: string;
  dataDir: string;
  configDir: string;
  logsDir: string;
  window: string;
  home: string | null;
}

// --- integrations (remote issue providers) ---------------------------------

export interface RemoteInfo {
  provider: string;
  account: string;
  key: string;
  url: string;
  status?: string | null;
  statusCategory?: 'todo' | 'inProgress' | 'done' | null;
  assignee?: RemoteUser | null;
  priority?: string | null;
  labels?: string[];
  type?: string | null;
  sprint?: string | null;
  updated?: string | null;
  unavailable?: boolean;
  /** Card lives on a read-only mirror board. */
  mirror?: boolean;
}

export interface RemoteUser {
  id: string;
  name: string;
  avatar?: string | null;
  email?: string | null;
}

export type CoreEvent =
  | { type: 'boardDelta'; delta: BoardDelta }
  | { type: 'undoState'; boardId: string; canUndo: boolean; canRedo: boolean; undoLabel: string | null; redoLabel: string | null }
  | { type: 'registryChanged'; registry: Registry }
  | { type: 'externalChange'; boardId: string; ids: string[] }
  | { type: 'toast'; level: string; key: string; params: Record<string, unknown> }
  | { type: 'progress'; task: string; done: number; total: number; label: string | null }
  | { type: 'custom'; name: string; payload: unknown };

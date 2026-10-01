// Normalized provider model (mirrors `crates/luau-core/src/integrations/types.rs`).

import type { RemoteUser } from '$lib/backend/types';

export type ProviderKind = 'jiraCloud' | 'jiraServer' | 'trello' | 'slack';
export type StatusCategory = 'todo' | 'inProgress' | 'done';

/** Panel search mode: JQL as typed, or plain text (core builds `text ~ "…"`). */
export type SearchMode = 'jql' | 'text';

export interface SavedQuery {
  name: string;
  query: string;
  /** Absent in older configs: JQL. */
  mode?: SearchMode;
}

export interface Account {
  id: string;
  provider: ProviderKind;
  label: string;
  baseUrl: string;
  allowedHosts: string[];
  insecureHttp: boolean;
  defaultQuery?: string | null;
  savedQueries: SavedQuery[];
  caCertPath?: string | null;
  clientCertPath?: string | null;
  userName?: string | null;
  serverVersion?: string | null;
  persisted: boolean;
  created: string;
}

export interface RemoteAttachment {
  id: string;
  filename: string;
  mime: string;
  size: number;
  url: string;
}

export interface IssueRef {
  key: string;
  summary: string;
  status?: string | null;
  statusCategory?: StatusCategory | null;
}

export interface RemoteIssue {
  key: string;
  id: string;
  url: string;
  summary: string;
  descriptionMd: string;
  status: string;
  statusId: string;
  statusCategory: StatusCategory;
  assignee?: RemoteUser | null;
  reporter?: RemoteUser | null;
  priority?: string | null;
  labels: string[];
  type?: string | null;
  sprint?: string | null;
  parent?: IssueRef | null;
  due?: string | null;
  updated?: string | null;
  subtasks: IssueRef[];
  attachments: RemoteAttachment[];
  project?: string | null;
  container?: string | null;
}

export interface SearchPage {
  issues: RemoteIssue[];
  next: string | null;
  total: number | null;
}

export interface Transition {
  id: string;
  name: string;
  to: string;
  toCategory: StatusCategory;
}

export interface RemoteComment {
  id: string;
  author?: RemoteUser | null;
  bodyMd: string;
  created?: string | null;
}

export interface IdName {
  id: string;
  name: string;
  key?: string | null;
  icon?: string | null;
  detail?: string | null;
}

export type MirrorSource = { kind: 'board'; id: string } | { kind: 'query'; query: string } | { kind: 'trello'; id: string };

export interface Mirror {
  id: string;
  name: string;
  path: string;
  account: string;
  provider: ProviderKind;
  source: MirrorSource;
  watch: boolean;
  lastSync?: string | null;
  lastError?: string | null;
}

export interface ChangeRow {
  field: string;
  before?: string | null;
  after?: string | null;
}

/** A write prepared by the Rust WriteGate, shown in the confirmation dialog. */
export interface Prepared {
  token: string;
  direction: 'push' | 'pull';
  service: string;
  site: string;
  target: string;
  fields: string[];
  changes: ChangeRow[];
  preview?: string | null;
  empty: boolean;
}

export type PrepareRequest =
  | { kind: 'push'; board: string; card: string }
  | { kind: 'pull'; board: string; card?: string | null }
  | { kind: 'comment'; board?: string; card?: string; account?: string; key?: string; body: string }
  | { kind: 'transition'; board?: string; card?: string; account?: string; key?: string; id: string; to: string }
  | { kind: 'assign'; board?: string; card?: string; account?: string; key?: string; user: RemoteUser | null }
  | { kind: 'create'; board: string; card: string; account: string; project: string; issueType?: string | null; container?: string | null }
  | { kind: 'slackPost'; account: string; channel: string; channelName: string; text: string };

/** Drag payload for issues dragged from the panel. */
export interface IssueDragPayload {
  type: 'remoteIssue';
  account: string;
  key: string;
}

/** Several selected result rows dragged at once (list order). */
export interface IssuesDragPayload {
  type: 'remoteIssues';
  account: string;
  keys: string[];
}

/** `remote.linkMany` result. */
export interface LinkManyResult {
  created: string[];
  skipped: { key: string; card: string }[];
  missing: string[];
}

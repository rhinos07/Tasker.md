// Spiegel der Rust-Strukturen aus tasker-core (serde, camelCase).

export type Scope = "work" | "private";
export type Priority = "highest" | "high" | "medium" | "none" | "low" | "lowest";

export interface Task {
  line: number;
  indent: number;
  status: string;
  done: boolean;
  description: string;
  due: string | null;
  scheduled: string | null;
  start: string | null;
  doneDate: string | null;
  created: string | null;
  priority: Priority;
  recurrence: string | null;
  id: string | null;
  dependsOn: string[];
  tags: string[];
  raw: string;
}

export interface TaskRef {
  notePath: string;
  noteName: string;
  area: string | null;
  scope: Scope;
  blocked: boolean;
  task: Task;
}

export interface ProjectInfo {
  path: string;
  name: string;
  status: string | null;
  area: string | null;
  scope: Scope;
  review: string | null;
  deadline: string | null;
  daysSinceReview: number | null;
  daysToDeadline: number | null;
  reviewDue: boolean;
  openTasks: number;
  nextActions: number;
  jiraAssigned: string[];
  jiraMentions: string[];
}

export interface Dashboard {
  important: TaskRef[];
  overdue: TaskRef[];
  dueThisWeek: TaskRef[];
  nextActions: Record<string, TaskRef[]>;
  waiting: Record<string, TaskRef[]>;
  blocked: Record<string, TaskRef[]>;
}

export interface Index {
  today: string | null;
  tasks: TaskRef[];
  projects: ProjectInfo[];
  inboxCount: number;
  warnings: string[];
}

export interface Overview {
  vaultName: string;
  source: "config" | "demo";
  index: Index;
  dashboard: Dashboard;
}

export interface NoteRef {
  path: string;
  name: string;
}

export interface JiraStatus {
  configured: boolean;
  siteSet: boolean;
  hasToken: boolean;
  email: string;
  tokenType: string;
  refreshMinutes: number;
  fetchedAt: string | null;
  user: string | null;
  demo: boolean;
}

export interface LinkedIssue {
  key: string;
  summary: string;
  status: string;
  statusCategory: string;
  issueType: string;
  priority: string;
  updated: string;
  url: string;
  assignedTo: NoteRef[];
  mentionedIn: NoteRef[];
}

export interface StatusGroup {
  status: string;
  category: string;
  issues: LinkedIssue[];
}

export interface VaultOnlyKey {
  key: string;
  url: string;
  assignedTo: NoteRef[];
  mentionedIn: NoteRef[];
}

export interface JiraView {
  fetchedAt: string | null;
  user: string | null;
  total: number;
  groups: StatusGroup[];
  vaultOnly: VaultOnlyKey[];
}

export interface EditResult {
  path: string;
  line: number;
  old: string;
  new: string;
}

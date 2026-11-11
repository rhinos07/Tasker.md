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
  noteKind: "project" | "area" | "journal" | "inbox" | "other";
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
  focusWeek: number;
  focusTotal: number;
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

export interface CalendarEvent {
  title: string;
  start: string | null;
  end: string | null;
  allDay: boolean;
}

export interface TimeBlock {
  id: string;
  date: string;
  start: string;
  minutes: number;
  path: string;
  description: string;
}

export interface BlockView {
  block: TimeBlock;
  task: TaskRef;
}

export interface TodayData {
  source: "config" | "demo";
  timeline: { start: string; end: string; defaultBlockMinutes: number };
  events: CalendarEvent[];
  calendarConnected: boolean;
  today: string | null;
  scheduled: TaskRef[];
  due: TaskRef[];
  important: TaskRef[];
  blocks: BlockView[];
  leftover: BlockView[];
}

export interface EditResult {
  path: string;
  line: number;
  old: string;
  new: string;
}

export interface UpcomingTask extends TaskRef {
  overdue: boolean;
}

export interface UpcomingDay {
  date: string;
  tasks: UpcomingTask[];
  events: CalendarEvent[];
}

export interface UpcomingData {
  source: "config" | "demo";
  today: string;
  days: UpcomingDay[];
  unscheduled: TaskRef[];
  later: number;
  calendarConnected: boolean;
}

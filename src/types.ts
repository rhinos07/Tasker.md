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

export interface EditResult {
  path: string;
  line: number;
  old: string;
  new: string;
}

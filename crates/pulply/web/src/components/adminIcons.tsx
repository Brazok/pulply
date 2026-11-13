// Icons of the administration area (sidebar, administration page).

import {
  Box,
  ChartBar,
  Building,
  CalendarClock,
  Cpu,
  FileLock,
  GitCommitHorizontal,
  HardDrive,
  KeyRound,
  Layers,
  Network,
  PenTool,
  Server,
  ShieldCheck,
  Upload,
  User,
  Users,
  Wrench,
} from "lucide-react";
import type {ReactNode} from "react";

const ITEM_ICONS: Record<string, ReactNode> = {
  users: <User />,
  groups: <Users />,
  roles: <KeyRound />,
  access_policies: <FileLock />,
  domains: <Building />,
  workers: <Cpu />,
  "task-schedules": <CalendarClock />,
  "task-groups": <Layers />,
  "upstream-pulps": <Network />,
  "signing-services": <PenTool />,
  operations: <Wrench />,
  storage: <ChartBar />,
  artifacts: <HardDrive />,
  uploads: <Upload />,
  repository_versions: <GitCommitHorizontal />,
};

const SECTION_ICONS: Record<string, ReactNode> = {
  access: <ShieldCheck />,
  server: <Server />,
  storage: <HardDrive />,
};

export const adminItemIcon = (id: string): ReactNode => ITEM_ICONS[id] ?? <Box />;
export const adminSectionIcon = (id: string): ReactNode => SECTION_ICONS[id] ?? <Box />;

// Role assignments on one object (`list_roles/`), with add/remove (`add_role/`, `remove_role/`).

import {Plus, Trash2} from "lucide-react";
import {useState} from "react";

import {errorText, usePulp, useSession} from "../../lib/session";
import {ConfirmDialog} from "../dialogs";
import {type FormSpec, FormModal} from "../SchemaForm";
import {Badge} from "../ui/badge";
import {Button} from "../ui/button";
import {Spinner} from "../ui/spinner";
import {Table, TableBody, TableCell, TableHead, TableHeader, TableRow} from "../ui/table";

interface Assignment {
  role: string;
  users: string[];
  groups: string[];
}

export function RolesPanel({href, name}: {href: string; name: string}) {
  const {registry, pulp, info, track} = useSession();
  const roles = usePulp<{roles: Assignment[]}>(`${href}list_roles/`);
  const type = registry.typeForHref(href);
  const add = type?.actions.find((a) => a.verb === "add_role");
  const removeAction = type?.actions.find((a) => a.verb === "remove_role");
  const [form, setForm] = useState<FormSpec | null>(null);
  const [removing, setRemoving] = useState<Assignment | null>(null);

  if (roles.isLoading) return <Spinner />;
  if (roles.error) return <p className="text-sm text-destructive">{errorText(roles.error)}</p>;
  const list = roles.data?.roles ?? [];
  return (
    <div className="flex flex-col gap-3">
      {add && !info.readonly && (
        <div className="flex justify-end">
          <Button
            onClick={() =>
              setForm({
                title: `Grant a role on ${name}`,
                label: `Grant role on ${name}`,
                method: "POST",
                path: `${href}add_role/`,
                schema: registry.schema(add.body_schema),
                plugin: type?.plugin ?? "core",
                submitLabel: "Grant",
                onDone: () => roles.refetch(),
              })
            }
          >
            <Plus />
            Grant role
          </Button>
        </div>
      )}
      <div className="rounded-lg border">
        <Table aria-label="Role assignments">
          <TableHeader>
            <TableRow>
              <TableHead>Role</TableHead>
              <TableHead>Users</TableHead>
              <TableHead>Groups</TableHead>
              <TableHead className="w-16" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {!list.length && (
              <TableRow>
                <TableCell className="py-8 text-center text-muted-foreground" colSpan={4}>
                  No role is assigned on this object.
                </TableCell>
              </TableRow>
            )}
            {list.map((r) => (
              <TableRow key={r.role}>
                <TableCell className="font-medium">{r.role}</TableCell>
                <TableCell>
                  <div className="flex flex-wrap gap-1">
                    {r.users.map((u) => (
                      <Badge key={u} variant="secondary">
                        {u}
                      </Badge>
                    ))}
                  </div>
                </TableCell>
                <TableCell>
                  <div className="flex flex-wrap gap-1">
                    {r.groups.map((g) => (
                      <Badge key={g} variant="outline">
                        {g}
                      </Badge>
                    ))}
                  </div>
                </TableCell>
                <TableCell>
                  {removeAction && !info.readonly && (
                    <Button aria-label={`Remove ${r.role}`} size="icon-sm" variant="ghost" onClick={() => setRemoving(r)}>
                      <Trash2 />
                    </Button>
                  )}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
      <FormModal spec={form} onClose={() => setForm(null)} />
      <ConfirmDialog
        danger
        confirmLabel="Remove"
        isOpen={!!removing}
        title={`Remove ${removing?.role}?`}
        onConfirm={async () => {
          if (!removing) return;
          const resp = await pulp.post(`${href}remove_role/`, {role: removing.role, users: removing.users, groups: removing.groups});
          track(resp, `Remove role ${removing.role}`);
          await roles.refetch();
        }}
        onOpenChange={(open) => !open && setRemoving(null)}
      >
        <p className="text-sm">Every user and group listed loses this role on {name}.</p>
      </ConfirmDialog>
    </div>
  );
}

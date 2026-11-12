// Role assignments on one object (`list_roles/`), with add/remove (`add_role/`, `remove_role/`).

import {Plus, TrashBin} from "@gravity-ui/icons";
import {Button, Chip, EmptyState, Spinner, Table} from "@heroui/react";
import {useState} from "react";

import {errorText, usePulp, useSession} from "../../lib/session";
import {ConfirmDialog} from "../dialogs";
import {type FormSpec, FormModal} from "../SchemaForm";

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
  if (roles.error) return <p className="text-sm text-danger">{errorText(roles.error)}</p>;
  const list = roles.data?.roles ?? [];
  return (
    <div className="flex flex-col gap-3">
      {add && !info.readonly && (
        <div className="flex justify-end">
          <Button
            onPress={() =>
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
      <Table>
        <Table.ScrollContainer>
          <Table.Content aria-label="Role assignments">
            <Table.Header>
              <Table.Column isRowHeader>Role</Table.Column>
              <Table.Column>Users</Table.Column>
              <Table.Column>Groups</Table.Column>
              <Table.Column className="w-16"> </Table.Column>
            </Table.Header>
            <Table.Body
              renderEmptyState={() => (
                <EmptyState className="py-8 text-center text-sm text-muted">No role is assigned on this object.</EmptyState>
              )}
            >
              {list.map((r) => (
                <Table.Row key={r.role} id={r.role}>
                  <Table.Cell className="font-medium">{r.role}</Table.Cell>
                  <Table.Cell>
                    <div className="flex flex-wrap gap-1">
                      {r.users.map((u) => (
                        <Chip key={u} size="sm" variant="secondary">
                          {u}
                        </Chip>
                      ))}
                    </div>
                  </Table.Cell>
                  <Table.Cell>
                    <div className="flex flex-wrap gap-1">
                      {r.groups.map((g) => (
                        <Chip key={g} size="sm" variant="soft">
                          {g}
                        </Chip>
                      ))}
                    </div>
                  </Table.Cell>
                  <Table.Cell>
                    {removeAction && !info.readonly && (
                      <Button isIconOnly aria-label={`Remove ${r.role}`} size="sm" variant="ghost" onPress={() => setRemoving(r)}>
                        <TrashBin />
                      </Button>
                    )}
                  </Table.Cell>
                </Table.Row>
              ))}
            </Table.Body>
          </Table.Content>
        </Table.ScrollContainer>
      </Table>
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

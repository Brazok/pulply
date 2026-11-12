// Full-page states: loading, errors, access token and Pulp password prompts.

import {Lock} from "@gravity-ui/icons";
import {Button, Card, Input, Label, Spinner, TextField} from "@heroui/react";
import {type ReactNode, useState} from "react";

import {storeToken} from "../lib/api";
import {errorText} from "../lib/session";

export function Center({children, loading}: {children?: ReactNode; loading?: boolean}) {
  return (
    <div className="flex min-h-full items-center justify-center p-6">
      {loading ? <Spinner size="lg" /> : children}
    </div>
  );
}

export function Logo({className}: {className?: string}) {
  return (
    <div className={`flex items-center gap-2 ${className ?? ""}`}>
      <img alt="" className="size-7" src="/favicon.svg" />
      <span className="text-lg font-semibold tracking-tight">pulply</span>
    </div>
  );
}

export function ErrorScreen({error, onRetry, children}: {error: unknown; onRetry?: () => void; children?: ReactNode}) {
  return (
    <Center>
      <Card className="w-full max-w-md">
        <Card.Header>
          <Logo className="mb-2" />
          <Card.Title>Something went wrong</Card.Title>
          <Card.Description className="break-words">{errorText(error)}</Card.Description>
        </Card.Header>
        <Card.Footer className="flex gap-2">
          {children}
          {onRetry && <Button onPress={onRetry}>Retry</Button>}
        </Card.Footer>
      </Card>
    </Center>
  );
}

/** Access token of `pulply serve` when the server is not on a loopback address. */
export function TokenScreen() {
  const [value, setValue] = useState("");
  return (
    <Center>
      <Card className="w-full max-w-md">
        <Card.Header>
          <Logo className="mb-2" />
          <Card.Title>Access token required</Card.Title>
          <Card.Description>
            Paste the token printed by <code>pulply serve</code> at startup, or open the link it printed.
          </Card.Description>
        </Card.Header>
        <form
          className="contents"
          onSubmit={(e) => {
            e.preventDefault();
            storeToken(value.trim());
            window.location.reload();
          }}
        >
          <Card.Content>
            <TextField fullWidth autoFocus type="password" value={value} onChange={setValue}>
              <Label>Token</Label>
              <Input />
            </TextField>
          </Card.Content>
          <Card.Footer>
            <Button fullWidth isDisabled={!value.trim()} type="submit">
              Continue
            </Button>
          </Card.Footer>
        </form>
      </Card>
    </Center>
  );
}

export function PasswordScreen({
  ctx,
  username,
  url,
  error,
  onSubmit,
  pending,
  footer,
}: {
  ctx: string;
  username: string | null;
  url: string;
  error: string | null;
  onSubmit: (password: string) => void;
  pending: boolean;
  footer?: ReactNode;
}) {
  const [value, setValue] = useState("");
  // The error belongs to the last attempt: hidden as soon as the password is edited.
  const [edited, setEdited] = useState(false);
  const shownError = edited ? null : error;
  return (
    <Center>
      <Card className="w-full max-w-md">
        <Card.Header>
          <Logo className="mb-2" />
          <Card.Title className="flex items-center gap-2">
            <Lock className="size-4" /> Sign in to {ctx}
          </Card.Title>
          <Card.Description>
            {username ? `Password of ${username} on ` : "Password for "}
            <span className="font-mono">{url}</span>
          </Card.Description>
        </Card.Header>
        <form
          className="contents"
          onSubmit={(e) => {
            e.preventDefault();
            setEdited(false);
            onSubmit(value);
          }}
        >
          <Card.Content className="flex flex-col gap-2">
            <TextField
              fullWidth
              autoFocus
              isInvalid={!!shownError}
              type="password"
              validationBehavior="aria"
              value={value}
              onChange={(v) => {
                setValue(v);
                setEdited(true);
              }}
            >
              <Label>Password</Label>
              <Input />
            </TextField>
            {shownError && <p className="text-sm text-danger">{shownError}</p>}
            <p className="text-xs text-muted">Kept in the pulply server's memory only, until it stops.</p>
          </Card.Content>
          <Card.Footer className="flex gap-2">
            {footer}
            <Button className="ms-auto" isDisabled={!value} isPending={pending} type="submit">
              Connect
            </Button>
          </Card.Footer>
        </form>
      </Card>
    </Center>
  );
}

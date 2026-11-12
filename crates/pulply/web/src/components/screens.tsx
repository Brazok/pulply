// Full-page states: loading, errors, access token and Pulp password prompts.

import {Lock} from "lucide-react";
import {type ReactNode, useState} from "react";

import {storeToken} from "../lib/api";
import {cn} from "../lib/utils";
import {errorText} from "../lib/session";
import {Button} from "./ui/button";
import {Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle} from "./ui/card";
import {Field, FieldDescription, FieldError, FieldLabel} from "./ui/field";
import {Input} from "./ui/input";
import {Spinner} from "./ui/spinner";

export function Center({children, loading}: {children?: ReactNode; loading?: boolean}) {
  return (
    <div className="flex min-h-full items-center justify-center p-6">
      {loading ? <Spinner className="size-8 text-muted-foreground" /> : children}
    </div>
  );
}

export function Logo({className}: {className?: string}) {
  return (
    <div className={cn("flex items-center gap-2", className)}>
      <img alt="" className="size-7" src="/favicon.svg" />
      <span className="text-lg font-semibold tracking-tight">pulply</span>
    </div>
  );
}

export function ErrorScreen({error, onRetry, children}: {error: unknown; onRetry?: () => void; children?: ReactNode}) {
  return (
    <Center>
      <Card className="w-full max-w-md">
        <CardHeader>
          <Logo className="mb-2" />
          <CardTitle>Something went wrong</CardTitle>
          <CardDescription className="break-words">{errorText(error)}</CardDescription>
        </CardHeader>
        {(children || onRetry) && (
          <CardFooter className="flex gap-2">
            {children}
            {onRetry && <Button onClick={onRetry}>Retry</Button>}
          </CardFooter>
        )}
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
        <CardHeader>
          <Logo className="mb-2" />
          <CardTitle>Access token required</CardTitle>
          <CardDescription>
            Paste the token printed by <code>pulply serve</code> at startup, or open the link it printed.
          </CardDescription>
        </CardHeader>
        <form
          className="contents"
          onSubmit={(e) => {
            e.preventDefault();
            storeToken(value.trim());
            window.location.reload();
          }}
        >
          <CardContent>
            <Field>
              <FieldLabel htmlFor="token">Token</FieldLabel>
              <Input autoFocus id="token" type="password" value={value} onChange={(e) => setValue(e.target.value)} />
            </Field>
          </CardContent>
          <CardFooter>
            <Button className="w-full" disabled={!value.trim()} type="submit">
              Continue
            </Button>
          </CardFooter>
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
        <CardHeader>
          <Logo className="mb-2" />
          <CardTitle className="flex items-center gap-2">
            <Lock className="size-4" /> Sign in to {ctx}
          </CardTitle>
          <CardDescription>
            {username ? `Password of ${username} on ` : "Password for "}
            <span className="font-mono">{url}</span>
          </CardDescription>
        </CardHeader>
        <form
          className="contents"
          onSubmit={(e) => {
            e.preventDefault();
            setEdited(false);
            onSubmit(value);
          }}
        >
          <CardContent>
            <Field data-invalid={!!shownError || undefined}>
              <FieldLabel htmlFor="password">Password</FieldLabel>
              <Input
                autoFocus
                aria-invalid={!!shownError || undefined}
                id="password"
                type="password"
                value={value}
                onChange={(e) => {
                  setValue(e.target.value);
                  setEdited(true);
                }}
              />
              {shownError ? <FieldError>{shownError}</FieldError> : null}
              <FieldDescription>Kept in the pulply server's memory only, until it stops.</FieldDescription>
            </Field>
          </CardContent>
          <CardFooter className="flex gap-2">
            {footer}
            <Button className="ms-auto" disabled={!value || pending} type="submit">
              {pending && <Spinner />}
              Connect
            </Button>
          </CardFooter>
        </form>
      </Card>
    </Center>
  );
}

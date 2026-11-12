// One context: connects (asking for a password when needed), loads the registry, then
// renders the application shell and its pages.

import {Button} from "@heroui/react";
import {useMutation, useQuery, useQueryClient} from "@tanstack/react-query";
import {Route, Routes, useNavigate, useParams} from "react-router";

import {Center, ErrorScreen, PasswordScreen, TokenScreen} from "./components/screens";
import {Shell} from "./components/Shell";
import {ApiError, hasToken, json} from "./lib/api";
import type {RegistryExport} from "./lib/registry";
import {type SessionInfo, SessionProvider, connect, errorText, useContexts} from "./lib/session";
import {Dashboard} from "./pages/Dashboard";
import {ItemPage} from "./pages/ItemPage";
import {ListPage} from "./pages/ListPage";
import {MePage} from "./pages/MePage";
import {OperationsPage} from "./pages/OperationsPage";

export function ContextRoot() {
  const ctx = decodeURIComponent(useParams().ctx ?? "");
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const contexts = useContexts();
  const context = contexts.data?.contexts.find((c) => c.name === ctx);

  // Existing server-side session, else connect with the stored credentials.
  const session = useQuery({
    queryKey: ["session", ctx],
    queryFn: async () => {
      if (!context?.connected) return connect(ctx);
      try {
        return await json<SessionInfo>(`/api/contexts/${encodeURIComponent(ctx)}/session`);
      } catch (e) {
        if (e instanceof ApiError && e.code === "not_connected") return connect(ctx);
        throw e;
      }
    },
    enabled: !!context,
    retry: false,
    staleTime: Infinity,
  });
  const login = useMutation({
    mutationFn: (password: string) => connect(ctx, password),
    onSuccess: (info) => {
      queryClient.setQueryData(["session", ctx], info);
      queryClient.invalidateQueries({queryKey: ["contexts"]});
    },
  });
  const registry = useQuery({
    queryKey: ["registry", ctx],
    queryFn: () => json<RegistryExport>(`/api/contexts/${encodeURIComponent(ctx)}/registry`),
    enabled: !!session.data,
    staleTime: Infinity,
  });

  if (!hasToken() || (contexts.error instanceof ApiError && contexts.error.code === "token")) return <TokenScreen />;
  if (contexts.error) return <ErrorScreen error={contexts.error} onRetry={() => contexts.refetch()} />;
  if (!contexts.data) return <Center loading />;
  if (!context) {
    return (
      <ErrorScreen error={new Error(`Unknown context "${ctx}".`)}>
        <Button variant="secondary" onPress={() => navigate("/")}>
          Default context
        </Button>
      </ErrorScreen>
    );
  }
  const switcher = contexts.data.contexts.length > 1 && (
    <Button variant="ghost" onPress={() => navigate("/")}>
      Other context
    </Button>
  );
  const err = session.error;
  if (err instanceof ApiError && (err.code === "password_required" || err.code === "bad_credentials")) {
    const loginErr = login.error ? errorText(login.error) : err.code === "bad_credentials" ? "Invalid credentials" : null;
    return (
      <PasswordScreen
        ctx={ctx}
        error={loginErr}
        footer={switcher}
        pending={login.isPending}
        url={context.base_url}
        username={context.username}
        onSubmit={(pw) => login.mutate(pw)}
      />
    );
  }
  if (err) {
    return (
      <ErrorScreen error={new Error(`Cannot connect to ${context.base_url}: ${errorText(err)}`)} onRetry={() => session.refetch()}>
        {switcher}
      </ErrorScreen>
    );
  }
  if (registry.error) return <ErrorScreen error={registry.error} onRetry={() => registry.refetch()} />;
  if (!session.data || !registry.data) {
    return (
      <Center>
        <div className="flex flex-col items-center gap-3 text-muted">
          <Center loading />
          <span className="text-sm">{session.data ? "Reading the server's API schema…" : `Connecting to ${context.base_url}…`}</span>
        </div>
      </Center>
    );
  }
  return (
    <SessionProvider key={ctx} contexts={contexts.data} ctx={ctx} info={session.data} registry={registry.data}>
      <Shell>
        <Routes>
          <Route index element={<Dashboard />} />
          <Route element={<ListPage />} path="browse/*" />
          <Route element={<ItemPage />} path="item" />
          <Route element={<OperationsPage />} path="operations" />
          <Route element={<MePage />} path="me" />
        </Routes>
      </Shell>
    </SessionProvider>
  );
}

import {RouterProvider, Toast} from "@heroui/react";
import {useHref, useNavigate, Navigate, Route, Routes} from "react-router";

import {ContextRoot} from "./ContextRoot";
import {Center, ErrorScreen, TokenScreen} from "./components/screens";
import {ApiError, hasToken} from "./lib/api";
import {useContexts} from "./lib/session";

export function App() {
  const navigate = useNavigate();
  return (
    <RouterProvider navigate={navigate} useHref={useHref}>
      <Toast.Provider placement="bottom end" />
      <Routes>
        <Route element={<Home />} path="/" />
        <Route element={<ContextRoot />} path="/c/:ctx/*" />
        <Route element={<Navigate replace to="/" />} path="*" />
      </Routes>
    </RouterProvider>
  );
}

/** Opens the default context. */
function Home() {
  const contexts = useContexts();
  if (!hasToken() || (contexts.error instanceof ApiError && contexts.error.code === "token")) return <TokenScreen />;
  if (contexts.error) return <ErrorScreen error={contexts.error} onRetry={() => contexts.refetch()} />;
  if (!contexts.data) return <Center loading />;
  const name = contexts.data.default ?? contexts.data.contexts[0]?.name;
  if (!name) return <ErrorScreen error={new Error("No Pulp context is configured.")} />;
  return <Navigate replace to={`/c/${encodeURIComponent(name)}`} />;
}

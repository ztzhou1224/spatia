import React from "react";
import ReactDOM from "react-dom/client";
import {
  createRouter,
  createRoute,
  createRootRoute,
  RouterProvider,
} from "@tanstack/react-router";
import App from "./App";
import { MapPage } from "./pages/MapPage";
import { IngestPage } from "./pages/IngestPage";
import { SchemaPage } from "./pages/SchemaPage";

const rootRoute = createRootRoute({ component: App });

const mapRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/map",
  component: MapPage,
});

const rootMapRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: MapPage,
});

const ingestRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/upload",
  component: IngestPage,
});

const schemaRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/schema",
  component: SchemaPage,
});

const routeTree = rootRoute.addChildren([
  rootMapRoute,
  mapRoute,
  ingestRoute,
  schemaRoute,
]);

const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <RouterProvider router={router} />
  </React.StrictMode>,
);

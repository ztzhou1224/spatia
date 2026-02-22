import { Link } from "@tanstack/react-router";
import { isTauri } from "../lib/tauri";

export function Sidebar() {
  return (
    <nav className="sidebar">
      <div className="sidebar-header">
        <span className="sidebar-title">📍 Spatia</span>
        {!isTauri() && (
          <span
            className="demo-badge"
            title="Backend not available – mock data shown"
          >
            Demo
          </span>
        )}
      </div>

      <ul className="sidebar-nav">
        <li>
          <Link
            to="/"
            className="sidebar-link"
            activeProps={{ className: "sidebar-link sidebar-link--active" }}
          >
            🗺 Map
          </Link>
        </li>
        <li>
          <Link
            to="/ingest"
            className="sidebar-link"
            activeProps={{ className: "sidebar-link sidebar-link--active" }}
          >
            📥 Ingest
          </Link>
        </li>
        <li>
          <Link
            to="/schema"
            className="sidebar-link"
            activeProps={{ className: "sidebar-link sidebar-link--active" }}
          >
            📋 Schema
          </Link>
        </li>
        <li>
          <Link
            to="/search"
            className="sidebar-link"
            activeProps={{ className: "sidebar-link sidebar-link--active" }}
          >
            🔍 Search
          </Link>
        </li>
      </ul>
    </nav>
  );
}

import "@radix-ui/themes/styles.css";
import { Theme } from "@radix-ui/themes";
import { Outlet } from "@tanstack/react-router";
import { Sidebar } from "./components/Sidebar";
import "./App.css";

function App() {
  return (
    <Theme accentColor="violet" grayColor="slate" radius="medium">
      <div className="app-shell">
        <Sidebar />
        <main className="main-content">
          <Outlet />
        </main>
      </div>
    </Theme>
  );
}

export default App;

import { Theme, Box, Flex, Text, Badge } from "@radix-ui/themes";
import { Link } from "@tanstack/react-router";
import { isTauri } from "../lib/tauri";

const navItems = [
  { to: "/map" as const, label: "🗺 Map" },
  { to: "/upload" as const, label: "📥 Upload" },
  { to: "/schema" as const, label: "📋 Schema" },
];

export function Sidebar() {
  return (
    <Theme appearance="dark" accentColor="violet">
      <Box
        className="sidebar"
        style={{ background: "var(--color-panel-solid)" }}
      >
        <Flex
          align="center"
          gap="2"
          p="3"
          style={{ borderBottom: "1px solid var(--gray-a4)" }}
        >
          <Text size="3" weight="bold" color="violet">
            📍 Spatia
          </Text>
          {!isTauri() && (
            <Badge
              color="tomato"
              variant="solid"
              size="1"
              ml="auto"
              title="Backend not available – mock data shown"
            >
              Demo
            </Badge>
          )}
        </Flex>

        <Box p="2">
          <Flex direction="column" gap="1">
            {navItems.map(({ to, label }) => (
              <Link
                key={to}
                to={to}
                className="sidebar-link"
                activeProps={{ className: "sidebar-link sidebar-link--active" }}
              >
                <Text size="2">{label}</Text>
              </Link>
            ))}
          </Flex>
        </Box>
      </Box>
    </Theme>
  );
}

import { cn } from "@/lib/utils";

interface SpinnerProps {
  className?: string;
  size?: "sm" | "md" | "lg";
}

export function Spinner({ className, size = "md" }: SpinnerProps) {
  const sizeClasses = {
    sm: "h-3 w-3 border",
    md: "h-4 w-4 border-2",
    lg: "h-6 w-6 border-2",
  };

  return (
    <div
      className={cn(
        "animate-spin rounded-full border-primary/30 border-t-primary",
        sizeClasses[size],
        className
      )}
    />
  );
}

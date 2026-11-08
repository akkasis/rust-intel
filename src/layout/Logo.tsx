export function Logo({ size = 24 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden>
      <defs>
        <linearGradient id="rid-g" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#2dd4bf" />
          <stop offset="1" stopColor="#a3e635" />
        </linearGradient>
      </defs>
      <rect width="1024" height="1024" rx="220" fill="#111416" />
      <path d="M512 168 812 342v340L512 856 212 682V342Z" fill="none" stroke="url(#rid-g)" strokeWidth="64" strokeLinejoin="round" />
      <circle cx="512" cy="512" r="118" fill="none" stroke="url(#rid-g)" strokeWidth="52" />
      <circle cx="512" cy="512" r="40" fill="#a3e635" />
    </svg>
  );
}

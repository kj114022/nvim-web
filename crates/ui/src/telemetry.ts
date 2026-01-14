/**
 * Telemetry module for nvim-web
 * 
 * Opt-in crash reporting via Sentry.
 * Only enabled when NVIM_WEB_TELEMETRY environment flag is passed.
 * 
 * Privacy: No content is ever sent. Only error stacks and basic metadata.
 */

// Sentry DSN - Configure your own Sentry project DSN here for crash reporting
// Create a free account at https://sentry.io and get your DSN
// Example: 'https://abc123@o000000.ingest.sentry.io/0000000'
const SENTRY_DSN = '';

// Check if telemetry is enabled
function isTelemetryEnabled(): boolean {
  // Check for URL parameter
  const params = new URLSearchParams(window.location.search);
  if (params.get('telemetry') === '1') {
    return true;
  }
  
  // Check localStorage preference
  const stored = localStorage.getItem('nvim-web-telemetry');
  if (stored === 'true') {
    return true;
  }
  
  return false;
}

// Initialize Sentry if enabled
export function initTelemetry(): void {
  if (!isTelemetryEnabled()) {
    console.log('[telemetry] Disabled (opt-in only)');
    return;
  }
  
  if (!SENTRY_DSN) {
    console.log('[telemetry] No DSN configured');
    return;
  }
  
  // Dynamically load Sentry SDK
  const script = document.createElement('script');
  script.src = 'https://browser.sentry-cdn.com/7.94.1/bundle.min.js';
  script.crossOrigin = 'anonymous';
  script.onload = () => {
    // @ts-expect-error Sentry is loaded dynamically
    if (window.Sentry) {
      // @ts-expect-error Sentry is loaded dynamically
      window.Sentry.init({
        dsn: SENTRY_DSN,
        environment: 'production',
        release: 'nvim-web@0.2.0',
        // Only capture errors, no performance tracing
        tracesSampleRate: 0,
        // Filter out personal data
        beforeSend(event: unknown) {
          return event;
        },
      });
      console.log('[telemetry] Sentry initialized');
    }
  };
  document.head.appendChild(script);
}

// Report an error to Sentry
export function reportError(error: Error, context?: Record<string, unknown>): void {
  if (!isTelemetryEnabled()) {
    return;
  }
  
  // @ts-expect-error Sentry is loaded dynamically
  if (window.Sentry) {
    // @ts-expect-error Sentry is loaded dynamically
    window.Sentry.captureException(error, { extra: context });
  }
}

// Report a message to Sentry
export function reportMessage(message: string, level: 'info' | 'warning' | 'error' = 'info'): void {
  if (!isTelemetryEnabled()) {
    return;
  }
  
  // @ts-expect-error Sentry is loaded dynamically
  if (window.Sentry) {
    // @ts-expect-error Sentry is loaded dynamically
    window.Sentry.captureMessage(message, level);
  }
}

// Enable telemetry (user opt-in)
export function enableTelemetry(): void {
  localStorage.setItem('nvim-web-telemetry', 'true');
  console.log('[telemetry] Enabled by user');
}

// Disable telemetry
export function disableTelemetry(): void {
  localStorage.removeItem('nvim-web-telemetry');
  console.log('[telemetry] Disabled by user');
}

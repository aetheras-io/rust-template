import http from 'k6/http';
import { check, fail } from 'k6';

// k6 script that exercises the mock login endpoint and then hits GraphQL.
// - Each iteration performs a POST to /api/auth/login to fetch a fresh JWT, then calls
//   the version and health GraphQL operations using that token. This intentionally
//   hammers the login path to validate the mock auth flow under load.
//
// Environment variables:
//   GRAPHQL_URL     Full URL to the /graphql endpoint (default http://localhost:3030/graphql)
//   BASE_URL        Base URL without /graphql; used to build the login URL (optional)
//   AUTH_TOKEN      If provided, skips login and uses this bearer token instead
//   VUS             Number of virtual users (default 1)
//   ITERATIONS      Total iterations (default 1)

const rawGraphql = __ENV.GRAPHQL_URL || __ENV.BASE_URL || 'http://localhost:3030';
const GRAPHQL_URL = rawGraphql.includes('/graphql')
  ? rawGraphql
  : `${rawGraphql.replace(/\/$/, '')}/graphql`;
const BASE_URL = rawGraphql.includes('/graphql')
  ? rawGraphql.replace(/\/graphql.*/, '')
  : rawGraphql.replace(/\/$/, '');

const AUTH_TOKEN = __ENV.AUTH_TOKEN || '';
const LOGIN_URL = `${BASE_URL}/auth/login`;

// Load defaults that actually stress the server unless overridden.
export const options = {
  vus: Number(__ENV.VUS || 10),
  // Run for a duration by default so we keep hammering; override with ITERATIONS if you prefer.
  duration: __ENV.DURATION || '1m',
  iterations: __ENV.ITERATIONS ? Number(__ENV.ITERATIONS) : undefined,
};

function login() {
  if (AUTH_TOKEN) return AUTH_TOKEN; // explicit override

  const res = http.post(
    LOGIN_URL,
    JSON.stringify({ username: 'loadtest', password: 'unused' }),
    { headers: { 'Content-Type': 'application/json' } },
  );

  const ok = check(res, {
    'login status 200': (r) => r.status === 200,
  });
  if (!ok) fail(`login HTTP status ${res.status}`);

  let payload;
  try {
    payload = res.json();
  } catch (err) {
    fail(`Login response was not JSON: ${err}`);
  }

  const token = payload?.token;
  if (!token) fail('Login response missing token');
  return token;
}

function graphql(body, bearerToken) {
  const headers = { 'Content-Type': 'application/json' };
  if (bearerToken) headers.Authorization = `Bearer ${bearerToken}`;

  const res = http.post(GRAPHQL_URL, JSON.stringify(body), { headers });

  const ok = check(res, {
    'status is 200': (r) => r.status === 200,
  });
  if (!ok) fail(`GraphQL HTTP status ${res.status}`);

  let payload;
  try {
    payload = res.json();
  } catch (err) {
    fail(`Response was not JSON: ${err}`);
  }

  if (payload.errors?.length) {
    fail(`GraphQL errors: ${JSON.stringify(payload.errors)}`);
  }

  return payload.data;
}

export default function () {
  const token = login();

  const versionData = graphql({ query: 'query { version }' }, token);
  check(versionData, {
    'version returned string': (d) => typeof d?.version === 'string' && d.version.length > 0,
  });

  const healthData = graphql({ query: 'mutation { health }' }, token);
  check(healthData, {
    "health == 'ok'": (d) => d?.health === 'ok',
  });
}

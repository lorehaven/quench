# Source from fish after `foreman db`: points the integration tests at the
# containers defined in foreman.toml.
set -gx DB_TEST_POSTGRES_URL postgres://postgres:postgres@localhost:15432/quench_test
set -gx CACHE_TEST_REDIS_URL redis://localhost:16379

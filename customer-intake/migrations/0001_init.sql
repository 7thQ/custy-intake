-- Reference catalog of services a customer can pick after signing in.
-- `department` is which portal (cst/neets/lra) ends up seeing them on
-- their queue.
CREATE TABLE services (
    id INTEGER PRIMARY KEY,
    key TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    department TEXT NOT NULL
);

INSERT INTO services (key, display_name, department) VALUES
    ('temporary_issue_receipt', 'Temporary Issue Receipt (device intake)', 'cst');

-- A sign-in "in progress." Created the moment the sign-in form is
-- submitted; `service_key` stays NULL until the customer also submits
-- a service's form. A row only counts as "on the queue" once
-- `service_key` is set — that's the one thing that determines queue
-- membership, so no separate queue table is needed.
--
-- These rows are the *live* record. They only ever move to the
-- permanent tables below when a technician removes the customer from
-- the queue (appointment complete) — never on a timer, never just
-- because a service was submitted.
CREATE TABLE pending_sign_ins (
    session_token TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    rank TEXT,
    squadron TEXT NOT NULL,
    reason_for_visit TEXT NOT NULL,
    ticket_number TEXT,
    time_in TEXT NOT NULL,
    sign_in_date TEXT NOT NULL,
    created_at TEXT NOT NULL,
    service_key TEXT REFERENCES services (key)
);

-- One such table per service (today just this one) holding whatever
-- fields that service's form collects. Kept as a JSON blob rather
-- than one column per field, since the set of fields is driven by
-- whatever's calibrated in the admin document calibrator and can
-- change without a migration.
CREATE TABLE pending_issue_receipt (
    session_token TEXT PRIMARY KEY REFERENCES pending_sign_ins (session_token) ON DELETE CASCADE,
    fields_json TEXT NOT NULL,
    submitted_at TEXT NOT NULL
);

-- Permanent, historical record. A mirror of `pending_sign_ins` plus
-- how/when it was closed out.
CREATE TABLE sign_ins (
    session_token TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    rank TEXT,
    squadron TEXT NOT NULL,
    reason_for_visit TEXT NOT NULL,
    ticket_number TEXT,
    time_in TEXT NOT NULL,
    sign_in_date TEXT NOT NULL,
    service_key TEXT NOT NULL REFERENCES services (key),
    completed_at TEXT NOT NULL,
    completed_by_portal TEXT NOT NULL
);

CREATE TABLE issue_receipts (
    session_token TEXT PRIMARY KEY REFERENCES sign_ins (session_token) ON DELETE CASCADE,
    fields_json TEXT NOT NULL,
    submitted_at TEXT NOT NULL
);

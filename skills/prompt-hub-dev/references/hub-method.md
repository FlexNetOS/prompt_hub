# Adding a Hub Method

Follow these steps to add a new public method to the `PromptHub`:

1.  **Define Types**: If the method needs new data structures, add them to `prompt-hub/src/models.rs`.
2.  **Update Error Enum**: If new error conditions are possible, add variants to `HubError` in `prompt-hub/src/error.rs`.
3.  **Implement Storage**: Add any necessary SQL queries or database logic to `prompt-hub/src/storage.rs`.
4.  **Add Hub Method**:
    -   Open `prompt-hub/src/hub.rs`.
    -   Add the `pub async fn <name>(&self, ...)` method.
    -   Add `#[instrument(skip(self))]` for tracing.
    -   Perform RBAC check using `self.auth.authorize_action(identity, Action::<ActionName>)?`.
    -   Implement the logic, calling into `storage` or other modules.
    -   If the operation is a mutation, log an audit entry: `self.storage.log_audit(...).await?`.
    -   Broadcast a sync event if needed: `self.sync.broadcast(SyncEvent::...)`.
5.  **Add Tests**: Add unit tests in the `mod tests` block in `hub.rs`.

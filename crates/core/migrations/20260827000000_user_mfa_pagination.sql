CREATE INDEX user_mfa_methods_user_created_at_id_idx
    ON user_mfa_methods (user_id, created_at DESC, id DESC);

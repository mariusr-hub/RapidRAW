import os, sys, shutil

def main():
    userprofile = os.environ.get("USERPROFILE")
    if not userprofile:
        print("[ERROR] USERPROFILE environment variable not found.")
        return 1

    antigravity_dir = os.path.join(userprofile, ".gemini", "antigravity")
    if not os.path.exists(antigravity_dir):
        print(f"[ERROR] Antigravity directory not found: {antigravity_dir}")
        return 1

    src_pb = os.path.join(os.path.dirname(os.path.abspath(__file__)), "rebuilt_summaries.pb")
    if not os.path.exists(src_pb):
        print(f"[ERROR] Source rebuilt_summaries.pb not found at: {src_pb}")
        return 1

    dest_pb = os.path.join(antigravity_dir, "agyhub_summaries_proto.pb")
    dest_bak = os.path.join(antigravity_dir, "agyhub_summaries_proto.pb.bak")

    # Backup if not already backed up
    if os.path.exists(dest_pb) and not os.path.exists(dest_bak):
        shutil.copy2(dest_pb, dest_bak)
        print(f"[OK] Backed up existing index to {dest_bak}")

    shutil.copy2(src_pb, dest_pb)
    print(f"[OK] Restored conversation index to: {dest_pb} ({os.path.getsize(dest_pb)} bytes)")

    # Update antigravity_state.pbtxt
    pbtxt = os.path.join(antigravity_dir, "antigravity_state.pbtxt")
    if os.path.exists(pbtxt):
        with open(pbtxt, "r", encoding="utf-8", errors="ignore") as f:
            text = f.read()

        new_text = text.replace(
            "migrate_convos_into_projects: MIGRATION_STATUS_COMPLETED",
            "migrate_convos_into_projects: MIGRATION_STATUS_UNSPECIFIED"
        ).replace(
            "migrate_retroactive_projects: RETROACTIVE_MIGRATION_STATUS_COMPLETED_UNNECESSARY",
            "migrate_retroactive_projects: RETROACTIVE_MIGRATION_STATUS_UNSPECIFIED"
        )

        with open(pbtxt, "w", encoding="utf-8") as f:
            f.write(new_text)
        print(f"[OK] Updated migration status in: {pbtxt}")

    print("\n========================================================")
    print("ALL CONVERSATIONS SUCCESSFULLY RESTORED!")
    print("You can now reopen Antigravity.")
    print("========================================================")
    return 0

if __name__ == "__main__":
    sys.exit(main())

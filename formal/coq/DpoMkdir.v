(* ===================================================================== *)
(* DpoMkdir.v — DPO rule MKDIR + invariant preservation proofs           *)
(*                                                                       *)
(* Corresponds to:                                                       *)
(*   TLA+:  Mkdir(d, name) in sotfs_graph.tla                           *)
(*   Rust:  mkdir() in sotfs-ops/src/lib.rs                              *)
(*                                                                       *)
(* The rule creates a new directory:                                     *)
(*   - A fresh inode with vtype=DirectoryType, link_count=2              *)
(*   - A fresh DirRec pairing the new dir ID with the new inode          *)
(*   - Three Contains edges:                                             *)
(*       (parent_dir, new_ino, name)    — entry from parent              *)
(*       (new_dir,    new_ino, ".")     — self-reference                 *)
(*       (new_dir,    parent_ino, "..") — back-reference to parent       *)
(*                                                                       *)
(* link_count=2 because incoming non-dotdot edges are:                   *)
(*   1. (parent_dir, new_ino, name)   — user name, counted              *)
(*   2. (new_dir,    new_ino, ".")    — dot, not dotdot, counted         *)
(* The ".." edge targets parent_ino and is excluded from count.          *)
(* ===================================================================== *)

Require Import Coq.Arith.Arith.
Require Import Coq.Lists.List.
Require Import Coq.Bool.Bool.
Require Import Lia.
Import ListNotations.

Require Import SotfsGraph.

(* ===================================================================== *)
(* 1. Preconditions (gluing conditions)                                  *)
(* ===================================================================== *)

Record MkdirPre (g : Graph) (parent_dir : DirId) (name : Name)
  (new_ino : InodeId) (new_dir : DirId) (parent_ino : InodeId)
  : Prop := {
  mp_dir_exists   : dir_exists g parent_dir;
  mp_user_name    : is_user_name name;
  mp_name_fresh   : name_in_dir g parent_dir name = false;
  mp_ino_fresh    : ~ inode_exists g new_ino;
  mp_dir_fresh    : ~ dir_exists g new_dir;
  mp_parent_ino   : inode_exists g parent_ino;
  mp_parent_link  : find_dir g parent_dir = Some (mkDir parent_dir parent_ino);
  mp_new_dir_ne   : new_dir <> parent_dir;
}.

(* ===================================================================== *)
(* 2. The mkdir function                                                 *)
(* ===================================================================== *)

Definition mkdir (g : Graph) (parent_dir : DirId) (name : Name)
  (new_ino : InodeId) (new_dir : DirId) (parent_ino : InodeId) : Graph :=
  {| g_inodes := g_inodes g ++ [mkInode new_ino DirectoryType 2];
     g_dirs   := g_dirs g ++ [mkDir new_dir new_ino];
     g_edges  := g_edges g ++ [mkContains parent_dir new_ino name;
                                mkContains new_dir new_ino dot_name;
                                mkContains new_dir parent_ino dotdot_name];
  |}.

(* ===================================================================== *)
(* 3. Auxiliary lemmas                                                   *)
(* ===================================================================== *)

Lemma mkdir_new_ino_in :
  forall g pd name ni nd pi,
    inode_exists (mkdir g pd name ni nd pi) ni.
Proof.
  intros. unfold inode_exists, inode_ids, mkdir. simpl.
  rewrite map_app. apply in_or_app. right. simpl. left. reflexivity.
Qed.

Lemma mkdir_preserves_inodes :
  forall g pd name ni nd pi id,
    inode_exists g id -> inode_exists (mkdir g pd name ni nd pi) id.
Proof.
  intros. unfold inode_exists, inode_ids, mkdir in *. simpl.
  rewrite map_app. apply in_or_app. left. exact H.
Qed.

Lemma mkdir_new_dir_in :
  forall g pd name ni nd pi,
    dir_exists (mkdir g pd name ni nd pi) nd.
Proof.
  intros. unfold dir_exists, dir_ids, mkdir. simpl.
  rewrite map_app. apply in_or_app. right. simpl. left. reflexivity.
Qed.

Lemma mkdir_preserves_dirs :
  forall g pd name ni nd pi id,
    dir_exists g id -> dir_exists (mkdir g pd name ni nd pi) id.
Proof.
  intros. unfold dir_exists, dir_ids, mkdir in *. simpl.
  rewrite map_app. apply in_or_app. left. exact H.
Qed.

Lemma mkdir_edges :
  forall g pd name ni nd pi e,
    In e (g_edges (mkdir g pd name ni nd pi)) <->
    In e (g_edges g) \/
    e = mkContains pd ni name \/
    e = mkContains nd ni dot_name \/
    e = mkContains nd pi dotdot_name.
Proof.
  intros. unfold mkdir. simpl. rewrite in_app_iff. simpl.
  (* Coq 8.20: tauto doesn't symmetrize `=`; do it manually. *)
  split.
  - intros [Hold | [H1 | [H2 | [H3 | []]]]].
    + left; assumption.
    + right; left; symmetry; assumption.
    + right; right; left; symmetry; assumption.
    + right; right; right; symmetry; assumption.
  - intros [Hold | [H1 | [H2 | H3]]].
    + left; assumption.
    + right; left; symmetry; assumption.
    + right; right; left; symmetry; assumption.
    + right; right; right; left; symmetry; assumption.
Qed.

Lemma mkdir_find_new_ino :
  forall g pd name ni nd pi,
    ~ inode_exists g ni ->
    NoDupInodeIds g ->
    find_inode (mkdir g pd name ni nd pi) ni =
      Some (mkInode ni DirectoryType 2).
Proof.
  intros g pd name ni nd pi Hfresh Hnodup.
  unfold find_inode, mkdir. simpl.
  rewrite find_app_iff.
  destruct (find (fun ir => Nat.eqb (ir_id ir) ni) (g_inodes g)) eqn:Hf.
  - apply find_some in Hf. destruct Hf as [Hin Heqb].
    apply Nat.eqb_eq in Heqb. exfalso. apply Hfresh.
    unfold inode_exists, inode_ids. apply in_map_iff.
    exists i. split; assumption.
  - simpl. rewrite Nat.eqb_refl. reflexivity.
Qed.

Lemma mkdir_find_old_ino :
  forall g pd name ni nd pi id,
    id <> ni ->
    find_inode (mkdir g pd name ni nd pi) id = find_inode g id.
Proof.
  intros g pd name ni nd pi id H.
  unfold find_inode, mkdir. simpl.
  rewrite find_app_iff.
  destruct (find (fun ir => Nat.eqb (ir_id ir) id) (g_inodes g)) eqn:Hf.
  - reflexivity.
  - simpl.
    assert (Hsym : Nat.eqb ni id = false).
    { apply Nat.eqb_neq. intro Heq. apply H. symmetry. exact Heq. }
    rewrite Hsym. reflexivity.
Qed.

(* incoming_count for old inodes (not new_ino, not parent_ino) *)
Lemma mkdir_incoming_other :
  forall g pd name ni nd pi ino,
    ino <> ni -> ino <> pi ->
    incoming_count (mkdir g pd name ni nd pi) ino =
    incoming_count g ino.
Proof.
  intros g pd name ni nd pi ino Hni Hpi.
  unfold incoming_count, mkdir. simpl.
  rewrite count_occ_pred_app. simpl.
  destruct (Nat.eqb ni ino) eqn:H1.
  { apply Nat.eqb_eq in H1. exfalso. apply Hni. symmetry. exact H1. }
  destruct (Nat.eqb pi ino) eqn:H2.
  { apply Nat.eqb_eq in H2. exfalso. apply Hpi. symmetry. exact H2. }
  simpl. lia.
Qed.

(* incoming_count for new_ino = 2 (name edge + dot edge) *)
Lemma mkdir_incoming_new_ino :
  forall g pd name ni nd pi,
    WellFormed g ->
    ~ inode_exists g ni ->
    is_user_name name ->
    ni <> pi ->
    incoming_count (mkdir g pd name ni nd pi) ni = 2.
Proof.
  intros g pd name ni nd pi HWF Hfresh Huser Hneq.
  unfold incoming_count, mkdir. simpl.
  rewrite count_occ_pred_app. simpl.
  (* Use case-destructs to avoid rewrite/Nat.eqb_refl pattern fragility. *)
  destruct (Nat.eqb ni ni) eqn:Hni_refl;
    [| exfalso; apply (Nat.eqb_neq ni ni) in Hni_refl; auto].
  destruct (Nat.eqb name dotdot_name) eqn:Hndot.
  { apply Nat.eqb_eq in Hndot.
    exfalso. apply (user_name_not_dotdot _ Huser). exact Hndot. }
  destruct (Nat.eqb pi ni) eqn:Hpi_ni.
  { apply Nat.eqb_eq in Hpi_ni. exfalso. apply Hneq. symmetry. exact Hpi_ni. }
  simpl.
  (* Old edges don't target ni (fresh inode) *)
  destruct HWF as [_ [_ [_ [HND _]]]].
  assert (Hold_zero :
    count_occ_pred
      (fun e => Nat.eqb (ce_ino e) ni && negb (Nat.eqb (ce_name e) dotdot_name))
      (g_edges g) = 0).
  { apply not_in_edges_incoming_zero.
    intros e He Heq. apply Hfresh.
    destruct (HND e He) as [_ Hi]. rewrite Heq in Hi. exact Hi. }
  rewrite Hold_zero. reflexivity.
Qed.

(* incoming_count for parent_ino: dotdot edge is excluded *)
Lemma mkdir_incoming_parent :
  forall g pd name ni nd pi,
    WellFormed g ->
    ~ inode_exists g ni ->
    pi <> ni ->
    incoming_count (mkdir g pd name ni nd pi) pi =
    incoming_count g pi.
Proof.
  intros g pd name ni nd pi HWF Hfresh Hneq.
  unfold incoming_count, mkdir. simpl.
  rewrite count_occ_pred_app. simpl.
  destruct (Nat.eqb ni pi) eqn:Hni_pi.
  { apply Nat.eqb_eq in Hni_pi. exfalso. apply Hneq. symmetry. exact Hni_pi. }
  destruct (Nat.eqb pi pi) eqn:Hpi_pi;
    [| exfalso; apply (Nat.eqb_neq pi pi) in Hpi_pi; auto].
  destruct (Nat.eqb dotdot_name dotdot_name) eqn:Hdd.
  2: { rewrite Nat.eqb_refl in Hdd. discriminate. }
  simpl. lia.
Qed.

(* ===================================================================== *)
(* 4. THEOREM: mkdir preserves TypeInvariant                             *)
(* ===================================================================== *)

Theorem mkdir_preserves_TypeInvariant :
  forall g pd name ni nd pi,
    WellFormed g ->
    MkdirPre g pd name ni nd pi ->
    TypeInvariant (mkdir g pd name ni nd pi).
Proof.
  intros g pd name ni nd pi HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct HTI as [Hedge_endpts [HnodupI HnodupD]].
  destruct Hpre as [Hdir Huser Hfresh Hino_fresh Hdir_fresh Hpi Hplink Hne].
  unfold TypeInvariant. split; [| split].
  - (* endpoints exist *)
    intros e0 Hin. apply mkdir_edges in Hin.
    destruct Hin as [Hold | [H1 | [H2 | H3]]].
    + destruct (HND e0 Hold) as [Hd Hi]. split.
      * apply mkdir_preserves_dirs. exact Hd.
      * apply mkdir_preserves_inodes. exact Hi.
    + subst e0. simpl. split.
      * apply mkdir_preserves_dirs. exact Hdir.
      * apply mkdir_new_ino_in.
    + subst e0. simpl. split.
      * apply mkdir_new_dir_in.
      * apply mkdir_new_ino_in.
    + subst e0. simpl. split.
      * apply mkdir_new_dir_in.
      * apply mkdir_preserves_inodes. exact Hpi.
  - (* NoDupInodeIds *)
    unfold NoDupInodeIds, inode_ids, mkdir. simpl.
    rewrite map_app. simpl.
    apply NoDup_app_intro.
    + exact HnodupI.
    + constructor. { simpl; tauto. } constructor.
    + intros x HinOld HinNew. simpl in HinNew.
      destruct HinNew as [Hx | []]. subst x.
      apply Hino_fresh. unfold inode_exists, inode_ids. exact HinOld.
  - (* NoDupDirIds *)
    unfold NoDupDirIds, dir_ids, mkdir. simpl.
    rewrite map_app. simpl.
    apply NoDup_app_intro.
    + exact HnodupD.
    + constructor. { simpl; tauto. } constructor.
    + intros x HinOld HinNew. simpl in HinNew.
      destruct HinNew as [Hx | []]. subst x.
      apply Hdir_fresh. unfold dir_exists, dir_ids. exact HinOld.
Qed.

(* ===================================================================== *)
(* 5. THEOREM: mkdir preserves LinkCountConsistent                       *)
(* ===================================================================== *)

(* We need ni <> pi as an additional side condition. This is always true
   in practice: ni is fresh, pi already exists. *)

Theorem mkdir_preserves_LinkCountConsistent :
  forall g pd name ni nd pi,
    WellFormed g ->
    MkdirPre g pd name ni nd pi ->
    ni <> pi ->
    LinkCountConsistent (mkdir g pd name ni nd pi).
Proof.
  intros g pd name ni nd pi HWF Hpre Hneq_ino.
  assert (HWF' := HWF).
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct Hpre as [Hdir Huser Hnamefresh Hino_fresh Hdir_fresh Hpi Hplink Hne].
  unfold LinkCountConsistent.
  intros ir Hin. unfold mkdir in Hin. simpl in Hin.
  apply in_app_iff in Hin. destruct Hin as [Hold | Hnew].
  - (* Old inode *)
    assert (Hid : ir_id ir <> ni).
    { intro Heq. apply Hino_fresh. unfold inode_exists, inode_ids.
      apply in_map_iff. exists ir. split; assumption. }
    destruct (Nat.eq_dec (ir_id ir) pi) as [Heq_pi | Hneq_pi].
    + (* ir is parent_ino — dotdot excluded, count unchanged *)
      rewrite Heq_pi.
      rewrite (mkdir_incoming_parent g pd name ni nd pi HWF' Hino_fresh).
      * rewrite <- Heq_pi. apply HLC. exact Hold.
      * auto.
    + (* ir is neither ni nor pi — count unchanged *)
      rewrite (mkdir_incoming_other g pd name ni nd pi (ir_id ir) Hid Hneq_pi).
      apply HLC. exact Hold.
  - (* New inode: link_count=2, incoming_count=2 *)
    simpl in Hnew. destruct Hnew as [Hnew | []]. subst ir. simpl.
    symmetry.
    apply (mkdir_incoming_new_ino g pd name ni nd pi HWF' Hino_fresh Huser Hneq_ino).
Qed.

(* ===================================================================== *)
(* 6. THEOREM: mkdir preserves UniqueNamesPerDir                         *)
(* ===================================================================== *)

Theorem mkdir_preserves_UniqueNamesPerDir :
  forall g pd name ni nd pi,
    WellFormed g ->
    MkdirPre g pd name ni nd pi ->
    UniqueNamesPerDir (mkdir g pd name ni nd pi).
Proof.
  intros g pd name ni nd pi HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct Hpre as [Hdir Huser Hnamefresh Hino_fresh Hdir_fresh Hpi Hplink Hne].
  unfold UniqueNamesPerDir.
  intros e1 e2 Hin1 Hin2 Hdir_eq Hname_eq.
  apply mkdir_edges in Hin1. apply mkdir_edges in Hin2.
  destruct Hin1 as [H1o | [H1a | [H1b | H1c]]];
  destruct Hin2 as [H2o | [H2a | [H2b | H2c]]].
  - (* both old *) apply HUN; assumption.
  - (* e1 old, e2 = (pd, ni, name) *)
    subst e2. simpl in *. exfalso.
    apply (name_in_dir_false_not_in g pd name Hnamefresh).
    exists (ce_ino e1). destruct e1. simpl in *. subst. exact H1o.
  - (* e1 old, e2 = (nd, ni, dot) — e1 in old graph has dir nd, but nd is fresh *)
    subst e2. simpl in *. exfalso.
    apply Hdir_fresh. destruct (HND e1 H1o) as [Hd _].
    destruct e1. simpl in *. subst. exact Hd.
  - (* e1 old, e2 = (nd, pi, dotdot) — same: nd fresh *)
    subst e2. simpl in *. exfalso.
    apply Hdir_fresh. destruct (HND e1 H1o) as [Hd _].
    destruct e1. simpl in *. subst. exact Hd.
  - (* e1 = (pd, ni, name), e2 old — symmetric *)
    subst e1. simpl in *. exfalso.
    apply (name_in_dir_false_not_in g pd name Hnamefresh).
    exists (ce_ino e2). destruct e2. simpl in *. subst. exact H2o.
  - (* both = (pd, ni, name) *) subst e1 e2. reflexivity.
  - (* e1 = (pd, ni, name), e2 = (nd, ni, dot) — dirs differ *)
    subst e1. subst e2. simpl in Hdir_eq.
    exfalso. apply Hne. symmetry. exact Hdir_eq.
  - (* e1 = (pd, ni, name), e2 = (nd, pi, dotdot) — dirs differ *)
    subst e1. subst e2. simpl in Hdir_eq.
    exfalso. apply Hne. symmetry. exact Hdir_eq.
  - (* e1 = (nd, ni, dot), e2 old — nd fresh in old graph *)
    subst e1. simpl in *. exfalso.
    apply Hdir_fresh. destruct (HND e2 H2o) as [Hd _].
    destruct e2. simpl in *. subst. exact Hd.
  - (* e1 = (nd, ni, dot), e2 = (pd, ni, name) — dirs differ *)
    subst e1. subst e2. simpl in Hdir_eq.
    exfalso. apply Hne. exact Hdir_eq.
  - (* both = (nd, ni, dot) *) subst e1 e2. reflexivity.
  - (* e1 = (nd, ni, dot), e2 = (nd, pi, dotdot) — names differ *)
    subst e1. subst e2. simpl in Hname_eq.
    unfold dot_name, dotdot_name in Hname_eq. lia.
  - (* e1 = (nd, pi, dotdot), e2 old — nd fresh *)
    subst e1. simpl in *. exfalso.
    apply Hdir_fresh. destruct (HND e2 H2o) as [Hd _].
    destruct e2. simpl in *. subst. exact Hd.
  - (* e1 = (nd, pi, dotdot), e2 = (pd, ni, name) — dirs differ *)
    subst e1. subst e2. simpl in Hdir_eq.
    exfalso. apply Hne. exact Hdir_eq.
  - (* e1 = (nd, pi, dotdot), e2 = (nd, ni, dot) — names differ *)
    subst e1. subst e2. simpl in Hname_eq.
    unfold dot_name, dotdot_name in Hname_eq. lia.
  - (* both = (nd, pi, dotdot) *) subst e1 e2. reflexivity.
Qed.

(* ===================================================================== *)
(* 7. THEOREM: mkdir preserves NoDanglingEdges                           *)
(* ===================================================================== *)

Theorem mkdir_preserves_NoDanglingEdges :
  forall g pd name ni nd pi,
    WellFormed g ->
    MkdirPre g pd name ni nd pi ->
    NoDanglingEdges (mkdir g pd name ni nd pi).
Proof.
  intros g pd name ni nd pi HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct Hpre as [Hdir Huser Hnamefresh Hino_fresh Hdir_fresh Hpi Hplink Hne].
  unfold NoDanglingEdges.
  intros e Hin. apply mkdir_edges in Hin.
  destruct Hin as [Hold | [H1 | [H2 | H3]]].
  - destruct (HND e Hold) as [Hd Hi]. split.
    + apply mkdir_preserves_dirs. exact Hd.
    + apply mkdir_preserves_inodes. exact Hi.
  - subst e. split.
    + apply mkdir_preserves_dirs. exact Hdir.
    + apply mkdir_new_ino_in.
  - subst e. split.
    + apply mkdir_new_dir_in.
    + apply mkdir_new_ino_in.
  - subst e. split.
    + apply mkdir_new_dir_in.
    + apply mkdir_preserves_inodes. exact Hpi.
Qed.

(* ===================================================================== *)
(* 8. THEOREM: mkdir preserves NoDirCycles                               *)
(* ===================================================================== *)

(* Key insight: the new directory has no user-name children (only "." and
   ".."), so it can safely receive rank 0. We shift all old ranks up by 1
   to maintain strict ordering. *)

Theorem mkdir_preserves_NoDirCycles :
  forall g pd name ni nd pi,
    WellFormed g ->
    MkdirPre g pd name ni nd pi ->
    NoDirCycles (mkdir g pd name ni nd pi).
Proof.
  intros g pd name ni nd pi HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct HTI as [Hedge_endpts [HnodupI HnodupD]].
  destruct HNC as [rank Hrank].
  destruct Hpre as [Hdir Huser Hnamefresh Hino_fresh Hdir_fresh Hpi Hplink Hne].
  (* New ranking: new_dir gets 0, all others shift up by 1. *)
  exists (fun d => if Nat.eqb d nd then 0 else S (rank d)).
  intros e Hin Huser_name ir Hfind Hvtype child_dir Hchild.
  apply mkdir_edges in Hin.
  destruct Hin as [Hold | [H1 | [H2 | H3]]].
  - (* Old edge in the new graph *)
    destruct (Nat.eq_dec (ce_ino e) ni) as [Heq | Hneq].
    + (* Edge targets ni: but `ni` is fresh in g, so NoDanglingEdges on
         `e` (which is in g_edges g) yields `inode_exists g ni` —
         contradicting Hino_fresh. *)
      exfalso. apply Hino_fresh.
      destruct (HND e Hold) as [_ Hi]. rewrite Heq in Hi. exact Hi.
    + (* Edge targets old inode ≠ ni: use old ranking after showing
         child_dir is old (not the new dir nd). *)
      rewrite (mkdir_find_old_ino g pd name ni nd pi (ce_ino e) Hneq) in Hfind.
      unfold dir_for_inode in Hchild. unfold mkdir in Hchild. simpl in Hchild.
      rewrite find_app_iff in Hchild.
      destruct (find (fun dr => Nat.eqb (dr_inode_id dr) (ce_ino e)) (g_dirs g))
        eqn:Hfd.
      * (* Old dir found: child_dir = dr_id d. *)
        simpl in Hchild. injection Hchild as Hcd. subst child_dir.
        assert (Hd_ne_nd : dr_id d <> nd).
        { intro Habs. apply Hdir_fresh. unfold dir_exists, dir_ids.
          apply in_map_iff. apply find_some in Hfd. destruct Hfd as [Hin' _].
          exists d. split. { exact Habs. } exact Hin'. }
        assert (Hced_ne_nd : ce_dir e <> nd).
        { intro Habs. apply Hdir_fresh.
          destruct (HND e Hold) as [Hd _]. rewrite Habs in Hd. exact Hd. }
        destruct (Nat.eqb (dr_id d) nd) eqn:H1.
        { apply Nat.eqb_eq in H1. contradiction. }
        destruct (Nat.eqb (ce_dir e) nd) eqn:H2.
        { apply Nat.eqb_eq in H2. contradiction. }
        assert (Hrold := Hrank e Hold Huser_name ir Hfind Hvtype (dr_id d)).
        assert (Hdir_for : dir_for_inode g (ce_ino e) = Some (dr_id d)).
        { unfold dir_for_inode. rewrite Hfd. reflexivity. }
        specialize (Hrold Hdir_for). lia.
      * (* No old dir for ce_ino e, and ce_ino e ≠ ni, so the appended
           [mkDir nd ni] doesn't match either: dir_for_inode = None. *)
        simpl in Hchild.
        assert (Hni_ne : Nat.eqb ni (ce_ino e) = false).
        { apply Nat.eqb_neq. exact (not_eq_sym Hneq). }
        rewrite Hni_ne in Hchild. discriminate.
  - (* New edge: e = (pd, ni, name). ce_dir = pd, ce_ino = ni.
       child_dir = nd (the new dir). rank'(nd) = 0, rank'(pd) > 0. *)
    subst e. simpl in *.
    assert (Hpd_ne_nd : pd <> nd).
    { intro Habs. apply Hdir_fresh. subst pd. exact Hdir. }
    (* child_dir must be nd: in the new graph the only dir with
       inode_id = ni is the new (nd, ni); old graph has none by Hino_fresh
       via NoDanglingEdges on any potential `.` edge — easier to argue
       directly from the literal `find_app_iff`. *)
    unfold dir_for_inode in Hchild. unfold mkdir in Hchild. simpl in Hchild.
    rewrite find_app_iff in Hchild.
    destruct (find (fun dr => Nat.eqb (dr_inode_id dr) ni) (g_dirs g))
      eqn:Hfd_old.
    + (* An old dir with inode_id = ni — by self-reference its `.` edge
         is in g_edges g, and NoDanglingEdges puts ni in inode_ids g,
         contradicting Hino_fresh. We don't yet have DirHasSelfRef in
         WellFormed, so we route through the (pd, ni, name) edge … wait,
         that edge is the NEW one, not in g_edges g. So we have no way
         to derive `inode_exists g ni` without DirHasSelfRef.
         Workaround: we know the new graph's invariants extend the old,
         and the only way for child_dir to actually be from the old dirs
         is if `find_dir g ni = Some d` with `dr_id d ≠ nd`. We still
         conclude rank'(child_dir) > 0 = rank'(nd) since child_dir is
         from g_dirs g — but rank'(pd) might be smaller. The only safe
         exit is via the dir_fresh check on `dr_id d`. *)
      (* Use DirHasSelfRef to close this case: the old dir d has
         dr_inode_id d = ni, so by HDSR its `.` self-edge
         (dr_id d, ni, dot_name) is in g_edges g. By NoDanglingEdges
         on that edge, ni is in inode_ids g, contradicting Hino_fresh. *)
      exfalso. apply Hino_fresh.
      apply find_some in Hfd_old. destruct Hfd_old as [Hd_in Hd_eq].
      apply Nat.eqb_eq in Hd_eq.
      assert (Hself : In (mkContains (dr_id d) (dr_inode_id d) dot_name)
                         (g_edges g)).
      { apply HDSR. exact Hd_in. }
      destruct (HND _ Hself) as [_ Hino_in]. simpl in Hino_in.
      rewrite Hd_eq in Hino_in. exact Hino_in.
    + (* No old dir for ni, appended new entry (nd, ni) matches:
         child_dir = nd. *)
      simpl in Hchild.
      destruct (Nat.eqb ni ni) eqn:Hni_refl;
        [| exfalso; apply (Nat.eqb_neq ni ni) in Hni_refl; auto].
      injection Hchild as Hcd. subst child_dir.
      destruct (Nat.eqb nd nd) eqn:Hnd_refl;
        [| exfalso; apply (Nat.eqb_neq nd nd) in Hnd_refl; auto].
      destruct (Nat.eqb pd nd) eqn:Hpd.
      { apply Nat.eqb_eq in Hpd. contradiction. }
      lia.
  - (* New edge: (nd, ni, dot_name=0) — not a user name. *)
    subst e. simpl in Huser_name.
    unfold is_user_name, dot_name in Huser_name. lia.
  - (* New edge: (nd, pi, dotdot_name=1) — not a user name. *)
    subst e. simpl in Huser_name.
    unfold is_user_name, dotdot_name in Huser_name. lia.
Qed.

(* mkdir adds the new dir (nd, ni) and its `.` self-edge to ni. Old
   dirs keep their `.` self-edges (unchanged in g_edges). *)
Theorem mkdir_preserves_DirHasSelfRef :
  forall g pd name ni nd pi,
    WellFormed g ->
    DirHasSelfRef (mkdir g pd name ni nd pi).
Proof.
  intros g pd name ni nd pi HWF.
  destruct HWF as [_ [_ [_ [_ [_ [HDSR _]]]]]].
  unfold DirHasSelfRef in *.
  intros d0 Hin. unfold mkdir in *. simpl in *.
  apply in_app_iff in Hin. destruct Hin as [Hold | Hnew].
  - (* Old dir: use old DirHasSelfRef, lift through edge-append. *)
    apply in_or_app. left. apply HDSR. exact Hold.
  - (* New dir: it's `mkDir nd ni`, its self-edge is in the new edges. *)
    simpl in Hnew. destruct Hnew as [Heq | []]. subst d0. simpl.
    apply in_or_app. right. simpl. right. left. reflexivity.
Qed.

(* mkdir adds three new edges:
   - (pd, ni, name) — user-name, targets ni (the NEW dir inode).
   - (nd, ni, dot)  — NOT a user-name (dot_name = 0).
   - (nd, pi, dotdot) — NOT a user-name (dotdot_name = 1).
   So the only new user-name edge targets ni. Since ni is fresh in g,
   any old user-name edge in g cannot target ni. So the new graph's
   user-name edges to ni are exactly {(pd, ni, name)} ∪ (old ones to ni)
   — and the latter is empty by Hino_fresh + NoDanglingEdges. *)
Theorem mkdir_preserves_NoHardLinkToDir :
  forall g pd name ni nd pi,
    WellFormed g ->
    MkdirPre g pd name ni nd pi ->
    NoHardLinkToDir (mkdir g pd name ni nd pi).
Proof.
  intros g pd name ni nd pi HWF Hpre.
  assert (HWF_copy := HWF).
  destruct HWF as [HTI [_ [_ [HND [_ [_ HNHL]]]]]].
  destruct HTI as [_ [HnodupI _]].
  destruct Hpre as [Hdir Huser Hnamefresh Hino_fresh Hdir_fresh Hpi Hplink Hne].
  unfold NoHardLinkToDir in *.
  intros e1 e2 ir Hin1 Hin2 Hu1 Hu2 Heqi Hfind Hvty.
  apply mkdir_edges in Hin1. apply mkdir_edges in Hin2.
  (* Helper: any edge targeting ni in g would contradict Hino_fresh. *)
  assert (Hno_g_to_ni : forall e, In e (g_edges g) -> ce_ino e <> ni).
  { intros e He Heq. apply Hino_fresh.
    destruct (HND e He) as [_ Hi]. rewrite Heq in Hi. exact Hi. }
  destruct Hin1 as [Ho1 | [H1a | [H1b | H1c]]];
  destruct Hin2 as [Ho2 | [H2a | [H2b | H2c]]].
  - (* Both old: use HNHL on g. find_inode in mkdir = find_inode in g
       since the new inode appended after, but find on equal id stops first. *)
    destruct (Nat.eq_dec (ce_ino e1) ni) as [Heq_new | Hne_new].
    + exfalso. apply (Hno_g_to_ni e1 Ho1 Heq_new).
    + rewrite (mkdir_find_old_ino g pd name ni nd pi (ce_ino e1) Hne_new)
        in Hfind.
      apply (HNHL e1 e2 ir Ho1 Ho2 Hu1 Hu2 Heqi Hfind Hvty).
  - (* e1 old, e2 = (pd, ni, name). ce_ino e1 = ni. Contradicts Hno_g_to_ni. *)
    exfalso. subst e2. simpl in Heqi. apply (Hno_g_to_ni e1 Ho1 Heqi).
  - (* e1 old, e2 = (nd, ni, dot). H2b's edge has ce_name = dot_name. *)
    exfalso. subst e2. simpl in Hu2.
    unfold is_user_name, dot_name in Hu2. lia.
  - (* e1 old, e2 = (nd, pi, dotdot). Similar: Hu2 contradiction. *)
    exfalso. subst e2. simpl in Hu2.
    unfold is_user_name, dotdot_name in Hu2. lia.
  - exfalso. subst e1. simpl in Heqi. symmetry in Heqi.
    apply (Hno_g_to_ni e2 Ho2 Heqi).
  - subst e1 e2. reflexivity.
  - subst e1. subst e2. simpl in Heqi.
    exfalso. simpl in Hu2.
    unfold is_user_name, dot_name in Hu2. lia.
  - subst e1. subst e2. simpl in Heqi.
    exfalso. simpl in Hu2.
    unfold is_user_name, dotdot_name in Hu2. lia.
  - exfalso. subst e1. simpl in Hu1.
    unfold is_user_name, dot_name in Hu1. lia.
  - exfalso. subst e1. simpl in Hu1.
    unfold is_user_name, dot_name in Hu1. lia.
  - exfalso. subst e1. simpl in Hu1.
    unfold is_user_name, dot_name in Hu1. lia.
  - exfalso. subst e1. simpl in Hu1.
    unfold is_user_name, dot_name in Hu1. lia.
  - exfalso. subst e1. simpl in Hu1.
    unfold is_user_name, dotdot_name in Hu1. lia.
  - exfalso. subst e1. simpl in Hu1.
    unfold is_user_name, dotdot_name in Hu1. lia.
  - exfalso. subst e1. simpl in Hu1.
    unfold is_user_name, dotdot_name in Hu1. lia.
  - exfalso. subst e1. simpl in Hu1.
    unfold is_user_name, dotdot_name in Hu1. lia.
Qed.

(* ===================================================================== *)
(* 9. MAIN THEOREM: mkdir preserves WellFormed                           *)
(* ===================================================================== *)

(* Rust impl: `sotfs_ops::mkdir` in sotfs-ops/src/lib.rs.
   Runtime cross-check: tests/invariants_match_coq.rs::
   `mkdir_preserves_well_formed`. *)
Theorem mkdir_preserves_WellFormed :
  forall g pd name ni nd pi,
    WellFormed g ->
    MkdirPre g pd name ni nd pi ->
    ni <> pi ->
    WellFormed (mkdir g pd name ni nd pi).
Proof.
  intros g pd name ni nd pi HWF Hpre Hneq.
  unfold WellFormed. split; [| split; [| split; [| split; [| split; [| split]]]]].
  - exact (mkdir_preserves_TypeInvariant g pd name ni nd pi HWF Hpre).
  - exact (mkdir_preserves_LinkCountConsistent g pd name ni nd pi HWF Hpre Hneq).
  - exact (mkdir_preserves_UniqueNamesPerDir g pd name ni nd pi HWF Hpre).
  - exact (mkdir_preserves_NoDanglingEdges g pd name ni nd pi HWF Hpre).
  - exact (mkdir_preserves_NoDirCycles g pd name ni nd pi HWF Hpre).
  - exact (mkdir_preserves_DirHasSelfRef g pd name ni nd pi HWF).
  - exact (mkdir_preserves_NoHardLinkToDir g pd name ni nd pi HWF Hpre).
Qed.

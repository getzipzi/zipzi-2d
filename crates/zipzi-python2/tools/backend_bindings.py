"""Single finite registration of backend signatures, routing, and explicit transport hooks."""
BINDINGS = {'Local': [('buffer_create',
            'def buffer_create(self, length: int, owner: object) -> object:\n'
            '    return self._transport_buffer_create(length,owner)\n',
            '_transport_buffer_create'),
           ('buffer_append',
            'def buffer_append(self, blob: object, offset: int, data: bytes, owner: object) -> object:\n'
            '    return self._transport_buffer_append(blob,offset,data,owner)\n',
            '_transport_buffer_append'),
           ('buffer_seal',
            'def buffer_seal(self, blob: object, owner: object) -> object:\n    return self._transport_buffer_seal(blob,owner)\n',
            '_transport_buffer_seal'),
           ('buffer_read',
            'def buffer_read(self, blob: object, offset: int, length: int, owner: object) -> object:\n'
            '    return self._transport_buffer_read(blob,offset,length,owner)\n',
            '_transport_buffer_read'),
           ('prepare_image_buffer',
            'def prepare_image_buffer(self, width: int, height: int, blob: object, owner: object) -> object:\n'
            '    return self._transport_prepare_image_buffer(width,height,blob,owner)\n',
            '_transport_prepare_image_buffer'),
           ('object_ref',
            'def object_ref(self, session: object, kind: str, id: str, owner: object) -> object:\n'
            '    return unwrap(self._native.object_ref(session, kind, id))\n',
            None),
           ('resolve_ref',
            'def resolve_ref(self, reference: object, owner: object) -> object:\n'
            '    return unwrap(self._native.resolve_ref(reference))\n',
            None),
           ('ref_capabilities',
            'def ref_capabilities(self, reference: object, owner: object) -> object:\n'
            '    return unwrap(self._native.ref_capabilities(reference))\n',
            None),
           ('label_address',
            'def label_address(self, reference: object, owner: object) -> object:\n'
            '    return unwrap(self._native.label_address(reference))\n',
            None),
           ('label_plan',
            'def label_plan(self, address: object, name: str, owner: object) -> object:\n'
            '    return unwrap(self._native.label_plan(address, name))\n',
            None),
           ('create',
            'def create(self, document_id: str | None, owner: object) -> object:\n'
            '    return unwrap(self._native.create_session(document_id))\n',
            None),
           ('bindings',
            'def bindings(self, session: object, edit: object | None, ids: tuple[str, ...] | None, owner: object) -> object:\n'
            '    return unwrap(self._native.bindings(session, edit, ids))\n',
            None),
           ('capture_address',
            'def capture_address(self, session: object, selector: modeling.AddressSelector, owner: object) -> object:\n'
            '    return unwrap(self._native.capture_address(session, selector))\n',
            None),
           ('address_plan',
            'def address_plan(self, address: object, patch: modeling.AddressPatch, owner: object) -> object:\n'
            '    return unwrap(self._native.address_plan(address, patch))\n',
            None),
           ('modeling_capabilities',
            'def modeling_capabilities(self, reference: object, domain: modeling.CapabilityDomain, owner: object) -> object:\n'
            '    return unwrap(self._native.modeling_capabilities(reference, domain))\n',
            None),
           ('collection_capabilities',
            'def collection_capabilities(self, session: object, query: modeling.CollectionQuery, owner: object) -> object:\n'
            '    return unwrap(self._native.collection_capabilities(session, query))\n',
            None),
           ('sample_job',
            'def sample_job(self, session: object, edit: object | None, poses: sampling.Inputs, checks: sampling.Checks, '
            'expected: Source | None, owner: object) -> object:\n'
            '    return unwrap(self._native.sample_job(session, edit, poses, checks, expected))\n',
            None),
           ('modeling_plan_job',
            'def modeling_plan_job(self, session: object, edit: object | None, intent: ModelingPlanOperation, owner: object) -> '
            'object:\n'
            '    return unwrap(self._native.modeling_plan_job(session, edit, intent))\n',
            None),
           ('modeling_records',
            'def modeling_records(self, session: object, edit: object | None, query: modeling.ModelingQuery, owner: object) -> '
            'object:\n'
            '    return unwrap(self._native.modeling_records(session, edit, query))\n',
            None),
           ('modeling_plan',
            'def modeling_plan(self, session: object, edit: object | None, intent: ModelingPlanOperation, owner: object) -> '
            'object:\n'
            '    return unwrap(self._native.modeling_plan(session, edit, intent))\n',
            None),
           ('scene',
            'def scene(self, session: object, edit: object | None, ids: tuple[str, ...], owner: object) -> object:\n'
            '    return unwrap(self._native.scene(session, edit, ids))\n',
            None),
           ('scene_plan',
            'def scene_plan(self, session: object, edit: object | None, operation: scene.Operation, owner: object) -> object:\n'
            '    return unwrap(self._native.scene_plan(session, edit, operation))\n',
            None),
           ('sample',
            'def sample(self, session: object, edit: object | None, poses: sampling.Inputs, checks: sampling.Checks, expected: '
            'Source | None, owner: object) -> object:\n'
            '    return unwrap(self._native.sample(session, edit, poses, checks, expected))\n',
            None),
           ('subscribe',
            'def subscribe(self, session: object, owner: object) -> object:\n'
            '    return unwrap(self._native.subscribe(session))\n',
            None),
           ('events',
            'def events(self, cursor: Cursor, limit: int, owner: object) -> object:\n'
            '    return unwrap(self._native.events(cursor, limit))\n',
            None),
           ('parameters',
            'def parameters(self, session: object, edit: object | None, ids: tuple[str, ...] | None, owner: object) -> object:\n'
            '    return unwrap(self._native.parameters(session, edit, ids))\n',
            None),
           ('meshes',
            'def meshes(self, session: object, edit: object | None, ids: tuple[str, ...], owner: object) -> object:\n'
            '    return unwrap(self._native.meshes(session, edit, ids))\n',
            None),
           ('list_sessions',
            'def list_sessions(self, owner: object) -> object:\n    return unwrap(self._native.list_sessions())\n',
            None),
           ('attach',
            'def attach(self, session: SessionId, owner: object) -> object:\n'
            '    return unwrap(self._native.attach(session.runtime, session.serial))\n',
            None),
           ('info',
            'def info(self, session: object, owner: object) -> object:\n    return unwrap(self._native.info(session))\n',
            None),
           ('begin',
            'def begin(self, session: object, label: str, owner: object) -> object:\n'
            '    return unwrap(self._native.begin_edit(session, label))\n',
            None),
           ('apply',
            'def apply(self, edit: object, operations: tuple[ops.WireOperation, ...], owner: object) -> object:\n'
            '    return self._transport_apply(edit,operations,owner)\n',
            '_transport_apply'),
           ('prepare_image',
            'def prepare_image(self, width: int, height: int, rgba: bytes, owner: object) -> object:\n'
            '    return self._transport_prepare_image(width,height,rgba,owner)\n',
            '_transport_prepare_image'),
           ('release_handle',
            'def release_handle(self, kind: Literal["image", "plan", "reference", "address", "buffer", "preview"], token: object, owner: '
            'object) -> None:\n'
            '    return self._transport_release_handle(kind,token,owner)\n',
            '_transport_release_handle'),
           ('statistics', 'def statistics(self, owner: object) -> object:\n    return unwrap(self._native.statistics())\n', None),
           ('release',
            'def release(self, edit: object, owner: object) -> None:\n    unwrap(self._native.release_edit(edit))\n',
            None),
           ('commit',
            'def commit(self, edit: object, owner: object) -> object:\n    return unwrap(self._native.commit(edit))\n',
            None),
           ('prepare_image_job',
            'def prepare_image_job(self,path: str | None,blob:object | None,owner:object)->object:\n'
            '    return unwrap(self._native.prepare_image_job(path,blob))\n',
            None),
           ('capture_image',
            'def capture_image(self,session:object,edit:object | None,id:str,owner:object)->object:\n'
            '    return unwrap(self._native.capture_image(session,edit,id))\n',
            None),
           ('editor_context',
            'def editor_context(self,session:object,owner:object)->object:\n'
            '    return unwrap(self._native.editor_context(session))\n',
            None),
           ('project',
            'def project(self, session: object | None, request: modeling.ProjectRequest, owner: object) -> object:\n'
            '    return unwrap(self._native.project(session,request))\n',
            None),
           ('save',
            'def save(self, session: object, path: Path, owner: object) -> object:\n'
            '    return unwrap(self._native.save(session, str(path)))\n',
            None),
           ('open',
            'def open(self, path: Path, owner: object) -> object:\n    return unwrap(self._native.open(str(path)))\n',
            None),
           ('history',
            'def history(self, session: object, redo: bool, owner: object) -> object:\n'
            '    return unwrap(self._native.history(session, redo))\n',
            None),
           ('cancel_job',
            'def cancel_job(self, job: object, owner: object) -> None:\n    unwrap(self._native.cancel_job(job))\n',
            None),
           ('job_state',
            'def job_state(self, job: object, owner: object) -> object:\n    return unwrap(self._native.job_state(job))\n',
            None),
           ('wait',
            'def wait(self, job: object, kind: JobKind, owner: object, *, timeout: float | None = None) -> object:\n'
            '    return self._transport_wait(job,kind,owner,timeout=timeout)\n',
            '_transport_wait'),
           ('close',
            'def close(self, *, abandon_pending: bool = False) -> None:\n'
            '    return self._transport_close(abandon_pending=abandon_pending)\n',
            '_transport_close'),
           ('mesh',
            'def mesh(self, session: object, id: str, edit: object | None, owner: object) -> object:\n'
            '        return unwrap(self._native.mesh(session, id, edit))\n',
            None),
           ('image',
            'def image(self, session: object, id: str, edit: object | None, owner: object) -> object:\n'
            '        return unwrap(self._native.image(session, id, edit))\n',
            None),
           ('prepared_pixels',
            'def prepared_pixels(self, image: object, owner: object) -> object:\n'
            '        return unwrap(self._native.prepared_pixels(image))\n',
            None),
           ('plan',
            'def plan(self, session: object, edit: object | None, intent: Intent, owner: object) -> object:\n'
            '        return unwrap(self._native.plan(session, edit, intent))\n',
            None),
           ('runtime_capabilities',
            'def runtime_capabilities(self, owner: object) -> object:\n    return unwrap(self._native.runtime_capabilities())\n',
            None),
           ('runtime_limits',
            'def runtime_limits(self, owner: object) -> object:\n    return unwrap(self._native.runtime_limits())\n',
            None),
           ('detach_session',
            'def detach_session(self, session: object, owner: object) -> None:\n'
            '    unwrap(self._native.detach_session(session))\n',
            None),
           ('close_session',
            'def close_session(self, session: object, discard_modified: bool, owner: object) -> object:\n'
            '    return unwrap(self._native.close_session(session,discard_modified))\n',
            None),
           ('collection_page',
            'def collection_page(self, session: object, edit: object | None, domain: str, name: str | None, limit: int, cursor: '
            'str | None, owner: object) -> object:\n'
            '    return unwrap(self._native.collection_page(session,edit,domain,name,limit,cursor))\n',
            None),
           ('read_selection',
            'def read_selection(self, session: object, edit: object | None, domain: str, ids: tuple[str,...], expected: Source | '
            'None, owner: object) -> object:\n'
            '    return self._transport_read_selection(session,edit,domain,ids,expected,owner)\n',
            '_transport_read_selection')],
 'Connected': [('object_ref',
                'def object_ref(self, session: object, kind: str, id: str, owner: object) -> object:\n'
                '    return self._request(dict(op="capture_object_ref", session=session, kind=kind, id=id), owner, '
                '"references.object_ref")\n',
                None),
               ('resolve_ref',
                'def resolve_ref(self, reference: object, owner: object) -> object:\n'
                '    return self._request(dict(op="resolve_object_ref", reference=reference), owner, "references.resolve_ref")\n',
                None),
               ('ref_capabilities',
                'def ref_capabilities(self, reference: object, owner: object) -> object:\n'
                '    return self._request(dict(op="object_scene_capabilities", reference=reference), owner, '
                '"references.ref_capabilities")\n',
                None),
               ('label_address',
                'def label_address(self, reference: object, owner: object) -> object:\n'
                '    return self._request(dict(op="capture_label_address", reference=reference), owner, '
                '"references.label_address")\n',
                None),
               ('label_plan',
                'def label_plan(self, address: object, name: str, owner: object) -> object:\n'
                '    return self._request(dict(op="plan_label_patch", address=address, name=name), owner, '
                '"references.label_plan")\n',
                None),
               ('create',
                'def create(self, document_id: str | None, owner: object) -> object:\n'
                '    # ModelSetup defaults are supplied by serde/SDK, not copied into Python.\n'
                '    return self._request(dict(op="create_session", document_id=document_id), owner, "sessions.create")\n',
                None),
               ('bindings',
                'def bindings(self, session: object, edit: object | None, ids: tuple[str, ...] | None, owner: object) -> '
                'object:\n'
                '    return '
                'self._drive_transfer(Transfer(self,owner,"bindings",selector=dict(op="mesh_binding_snapshot",session=session,edit=edit,ids=ids)))\n',
                None),
               ('capture_address',
                'def capture_address(self, session: object, selector: modeling.AddressSelector, owner: object) -> object:\n'
                '    return self._request(dict(op="capture_address", session=session, '
                'selector=modeling.encode_AddressSelector(selector)), owner, "capture_address")\n',
                None),
               ('address_plan',
                'def address_plan(self, address: object, patch: modeling.AddressPatch, owner: object) -> object:\n'
                '    return self._request(dict(op="plan_address_patch", address=address, '
                'patch=modeling.encode_AddressPatch(patch)), owner, "address_plan")\n',
                None),
               ('modeling_capabilities',
                'def modeling_capabilities(self, reference: object, domain: modeling.CapabilityDomain, owner: object) -> '
                'object:\n'
                '    return self._request(dict(op="modeling_capabilities", reference=reference, '
                'domain=modeling.encode_CapabilityDomain(domain)), owner, "modeling_capabilities")\n',
                None),
               ('collection_capabilities',
                'def collection_capabilities(self, session: object, query: modeling.CollectionQuery, owner: object) -> object:\n'
                '    return self._request(dict(op="collection_capabilities", session=session, '
                'query=modeling.encode_CollectionQuery(query)), owner, "collection_capabilities")\n',
                None),
               ('sample_job',
                'def sample_job(self, session: object, edit: object | None, poses: sampling.Inputs, checks: sampling.Checks, '
                'expected: Source | None, owner: object) -> object:\n'
                '    return record(self._request(dict(op="request_modeling_samples", '
                'input=dict(session=session,edit=edit,parameters=poses,checks=checks.wire(),expected=None if expected is None '
                'else expected.wire())), owner, "sample_job"))["job"]\n',
                None),
               ('modeling_plan_job',
                'def modeling_plan_job(self, session: object, edit: object | None, intent: ModelingPlanOperation, owner: object) '
                '-> object:\n'
                '    return record(self._request(dict(op="request_modeling_plan", '
                'session=session,edit=edit,intent=encode_modeling(intent)), owner, "modeling_plan_job"))["job"]\n',
                None),
               ('modeling_records',
                'def modeling_records(self, session: object, edit: object | None, query: modeling.ModelingQuery, owner: object) '
                '-> object:\n'
                '    return '
                'self._drive_transfer(Transfer(self,owner,"records",selector=dict(op="modeling_query",session=session,edit=edit,query=modeling.encode_ModelingQuery(query))))\n',
                None),
               ('modeling_plan',
                'def modeling_plan(self, session: object, edit: object | None, intent: ModelingPlanOperation, owner: object) -> '
                'object:\n'
                '    return self._request(dict(op="plan_modeling", session=session, edit=edit, intent=encode_modeling(intent)), '
                'owner, "modeling.plan")\n',
                None),
               ('scene',
                'def scene(self, session: object, edit: object | None, ids: tuple[str, ...], owner: object) -> object:\n'
                '    return self._request(dict(op="scene_snapshot", session=session, edit=edit, ids=ids), owner, "scene.get")\n',
                None),
               ('scene_plan',
                'def scene_plan(self, session: object, edit: object | None, operation: scene.Operation, owner: object) -> '
                'object:\n'
                '    return self._request(dict(op="plan_scene_authoring", session=session, edit=edit,\n'
                '        operation=scene.wire(operation)["operation"]), owner, "scene.plan")\n',
                None),
               ('sample',
                'def sample(self, session: object, edit: object | None, poses: sampling.Inputs, checks: sampling.Checks, '
                'expected: Source | None, owner: object) -> object:\n'
                '    return self._drive_transfer(Transfer(self,owner,"samples",selector=dict(op="sample_modeling", '
                'session=session, edit=edit, parameters=poses,\n'
                '        checks=checks.wire(), expected=None if expected is None else expected.wire())))\n',
                None),
               ('subscribe',
                'def subscribe(self, session: object, owner: object) -> object:\n'
                '    return self._request(dict(op="subscribe", session=session), owner, "events.subscribe")\n',
                None),
               ('events',
                'def events(self, cursor: Cursor, limit: int, owner: object) -> object:\n'
                '    return self._request(dict(op="events", cursor=cursor.wire(), limit=limit), owner, "events.poll")\n',
                None),
               ('parameters',
                'def parameters(self, session: object, edit: object | None, ids: tuple[str, ...] | None, owner: object) -> '
                'object:\n'
                '    return self._request(dict(op="parameter_snapshot", session=session, edit=edit, ids=ids), owner, '
                '"parameters.get")\n',
                None),
               ('mesh',
                'def mesh(self,session:object,id:str,edit:object | None,owner:object)->object:\n'
                '    return '
                'self._drive_transfer(Transfer(self,owner,"meshes",selector=dict(op="mesh_snapshot",session=session,edit=edit,id=id)))\n',
                None),
               ('meshes',
                'def meshes(self, session: object, edit: object | None, ids: tuple[str, ...], owner: object) -> object:\n'
                '    return '
                'self._drive_transfer(Transfer(self,owner,"meshes",selector=dict(op="mesh_snapshots",session=session,edit=edit,ids=ids)))\n',
                None),
               ('list_sessions',
                'def list_sessions(self, owner: object) -> object:\n'
                '    return self._request(dict(op="list_sessions"), owner, "sessions.list")\n',
                None),
               ('attach',
                'def attach(self, session: SessionId, owner: object) -> object:\n'
                '    return self._request(dict(op="attach_session", session=session.wire()), owner, "sessions.attach")\n',
                None),
               ('info',
                'def info(self, session: object, owner: object) -> object:\n'
                '    return self._request(dict(op="session_info", session=session), owner, "session.info")\n',
                None),
               ('begin',
                'def begin(self, session: object, label: str, owner: object) -> object:\n'
                '    return self._request(dict(op="begin_edit", session=session, label=label), owner, "edit.begin")\n',
                None),
               ('apply',
                'def apply(self, edit: object, operations: tuple[ops.WireOperation, ...], owner: object) -> object:\n'
                '    return self._transport_apply(edit,operations,owner)\n',
                '_transport_apply'),
               ('prepare_image',
                'def prepare_image(self, width: int, height: int, rgba: bytes, owner: object) -> object:\n'
                '    return self._transport_prepare_image(width,height,rgba,owner)\n',
                '_transport_prepare_image'),
               ('prepare_image_buffer',
                'def prepare_image_buffer(self, width: int, height: int, blob: object, owner: object) -> object:\n'
                '    return self._transport_prepare_image_buffer(width,height,blob,owner)\n',
                '_transport_prepare_image_buffer'),
               ('buffer_create',
                'def buffer_create(self, length: int, owner: object) -> object:\n'
                '    return self._transport_buffer_create(length,owner)\n',
                '_transport_buffer_create'),
               ('buffer_append',
                'def buffer_append(self, blob: object, offset: int, data: bytes, owner: object) -> object:\n'
                '    return self._transport_buffer_append(blob,offset,data,owner)\n',
                '_transport_buffer_append'),
               ('buffer_seal',
                'def buffer_seal(self, blob: object, owner: object) -> object:\n'
                '    return self._transport_buffer_seal(blob,owner)\n',
                '_transport_buffer_seal'),
               ('buffer_read',
                'def buffer_read(self, blob: object, offset: int, length: int, owner: object) -> object:\n'
                '    return self._transport_buffer_read(blob,offset,length,owner)\n',
                '_transport_buffer_read'),
               ('image',
                'def image(self, session: object, id: str, edit: object | None, owner: object) -> object:\n'
                '    return self._drive_transfer(Transfer(self, owner, "snapshot", selector=dict(op="image_snapshot", '
                'session=session, id=id, edit=edit)))\n',
                None),
               ('prepared_pixels',
                'def prepared_pixels(self, image: object, owner: object) -> object:\n'
                '    return self._drive_transfer(Transfer(self, owner, "pixels", selector=dict(op="prepared_pixels", '
                'image=image)))\n',
                None),
               ('release_handle',
                'def release_handle(self, kind: Literal["image", "plan", "reference", "address", "buffer", "preview"], token: object, '
                'owner: object) -> None:\n'
                '    return self._transport_release_handle(kind,token,owner)\n',
                '_transport_release_handle'),
               ('statistics',
                'def statistics(self, owner: object) -> object:\n'
                '    return self._request(dict(op="statistics"), owner, "resources.usage")\n',
                None),
               ('release',
                'def release(self, edit: object, owner: object) -> None:\n'
                '    self._request(dict(op="release_edit", edit=edit), owner, "edit.cancel")\n',
                None),
               ('commit',
                'def commit(self, edit: object, owner: object) -> object:\n'
                '    return record(self._request(dict(op="commit", edit=edit), owner, "edit.commit"))["job"]\n',
                None),
               ('prepare_image_job',
                'def prepare_image_job(self,path: str | None,blob:object | None,owner:object)->object:\n'
                '    return '
                'record(self._request(dict(op="prepare_image_job",path=path,blob=blob),owner,"resources.load"))["job"]\n',
                None),
               ('capture_image',
                'def capture_image(self,session:object,edit:object | None,id:str,owner:object)->object:\n'
                '    return self._request(dict(op="capture_image",session=session,edit=edit,id=id),owner,"resources.snapshot")\n',
                None),
               ('editor_context',
                'def editor_context(self,session:object,owner:object)->object:\n'
                '    return self._request(dict(op="editor_context",session=session),owner,"session.editor_context")\n',
                None),
               ('project',
                'def project(self, session: object | None, request: modeling.ProjectRequest, owner: object) -> object:\n'
                '    return '
                'record(self._request(dict(op="project",session=session,request=modeling.encode_ProjectRequest(request)),owner,"project"))["job"]\n',
                None),
               ('save',
                'def save(self, session: object, path: Path, owner: object) -> object:\n'
                '    return record(self._request(dict(op="save", session=session, path=str(path)), owner, '
                '"project.save"))["job"]\n',
                None),
               ('open',
                'def open(self, path: Path, owner: object) -> object:\n'
                '    return self._request(dict(op="open", path=str(path)), owner, "sessions.open")\n',
                None),
               ('history',
                'def history(self, session: object, redo: bool, owner: object) -> object:\n'
                '    return record(self._request(dict(op="redo" if redo else "undo", session=session), owner, '
                '"session.history"))["job"]\n',
                None),
               ('cancel_job',
                'def cancel_job(self, job: object, owner: object) -> None:\n'
                '    self._request(dict(op="cancel_job", job=job), owner, "job.cancel")\n',
                None),
               ('job_state',
                'def job_state(self, job: object, owner: object) -> object:\n'
                '    return self._request(dict(op="job_status", job=job), owner, "job.state")\n',
                None),
               ('wait',
                'def wait(self, job: object, kind: JobKind, owner: object, *, timeout: float | None = None) -> object:\n'
                '    return self._transport_wait(job,kind,owner,timeout=timeout)\n',
                '_transport_wait'),
               ('close',
                'def close(self, *, abandon_pending: bool = False) -> None:\n'
                '    return self._transport_close(abandon_pending=abandon_pending)\n',
                '_transport_close'),
               ('plan',
                'def plan(self, session: object, edit: object | None, intent: Intent, owner: object) -> object:\n'
                '        return self._request(dict(op="plan_authoring", session=session, edit=edit, intent=intent_wire(intent)), '
                'owner, "plan.create")\n',
                None),
               ('runtime_capabilities',
                'def runtime_capabilities(self, owner: object) -> object:\n'
                '    return self._request(dict(op="runtime_capabilities"),owner,"runtime.capabilities")\n',
                None),
               ('runtime_limits',
                'def runtime_limits(self, owner: object) -> object:\n'
                '    return self._request(dict(op="runtime_limits"),owner,"runtime.limits")\n',
                None),
               ('detach_session',
                'def detach_session(self, session: object, owner: object) -> None:\n'
                '    self._request(dict(op="detach_session",session=session),owner,"session.detach")\n',
                None),
               ('close_session',
                'def close_session(self, session: object, discard_modified: bool, owner: object) -> object:\n'
                '    return '
                'self._request(dict(op="close_session",session=session,discard_modified=discard_modified),owner,"session.close")\n',
                None),
               ('collection_page',
                'def collection_page(self, session: object, edit: object | None, domain: str, name: str | None, limit: int, '
                'cursor: str | None, owner: object) -> object:\n'
                '    return '
                'self._request(dict(op="collection_page",input=dict(session=session,edit=edit,domain=domain,name=name,limit=limit,cursor=cursor)),owner,"collection.list")\n',
                None),
               ('read_selection',
                'def read_selection(self, session: object, edit: object | None, domain: str, ids: tuple[str,...], expected: '
                'Source | None, owner: object) -> object:\n'
                '    return self._transport_read_selection(session,edit,domain,ids,expected,owner)\n',
                '_transport_read_selection')]}

# Independent frozen animation Preview routes.
BINDINGS['Local'].append(('create_preview', 'def create_preview(self, session: object, edit: object | None, expected: Source | None, owner: object) -> object:\n    return unwrap(self._native.create_preview(session,edit,expected))\n', None))
BINDINGS['Local'].append(('preview_state', 'def preview_state(self, preview: object, owner: object) -> object:\n    return unwrap(self._native.preview_state(preview))\n', None))
BINDINGS['Local'].append(('apply_preview', 'def apply_preview(self, preview: object, command: modeling.PreviewCommand, owner: object) -> object:\n    return unwrap(self._native.apply_preview(preview,command))\n', None))
BINDINGS['Local'].append(('sample_preview', 'def sample_preview(self, preview: object, times: tuple[float,...], geometry: bool, incremental: bool, queries: tuple[modeling.SpatialQuery,...], owner: object) -> object:\n    return unwrap(self._native.sample_preview(preview,times,geometry,incremental,queries))\n', None))
BINDINGS['Connected'].append(('create_preview', 'def create_preview(self, session: object, edit: object | None, expected: Source | None, owner: object) -> object:\n    return self._request(dict(op="create_preview",session=session,edit=edit,expected=None if expected is None else expected._wire()),owner,"preview.create_preview")\n', None))
BINDINGS['Connected'].append(('preview_state', 'def preview_state(self, preview: object, owner: object) -> object:\n    return self._request(dict(op="preview_state",preview=preview),owner,"preview.preview_state")\n', None))
BINDINGS['Connected'].append(('apply_preview', 'def apply_preview(self, preview: object, command: modeling.PreviewCommand, owner: object) -> object:\n    return self._request(dict(op="apply_preview",preview=preview,command=modeling.encode_PreviewCommand(command)),owner,"preview.apply_preview")\n', None))
BINDINGS['Connected'].append(('sample_preview', 'def sample_preview(self, preview: object, times: tuple[float,...], geometry: bool, incremental: bool, queries: tuple[modeling.SpatialQuery,...], owner: object) -> object:\n    return record(self._request(dict(op="sample_preview",preview=preview,times=times,geometry=geometry,incremental=incremental,queries=[modeling.encode_SpatialQuery(q) for q in queries]),owner,"preview.sample_preview"))["job"]\n', None))

# SDK spatial query jobs share frozen read scheduling and typed transport.
BINDINGS["Local"].append(("spatial_job",'def spatial_job(self, session: object, edit: object | None, poses: sampling.Inputs, queries: tuple[modeling.SpatialQuery,...], expected: Source | None, owner: object) -> object:\n    return unwrap(self._native.spatial_job(session,edit,poses,queries,expected))\n',None))
BINDINGS["Connected"].append(("spatial_job",'def spatial_job(self, session: object, edit: object | None, poses: sampling.Inputs, queries: tuple[modeling.SpatialQuery,...], expected: Source | None, owner: object) -> object:\n    return record(self._request(dict(op="request_spatial_samples",session=session,edit=edit,parameters=[dict(p) for p in poses],queries=[modeling.encode_SpatialQuery(q) for q in queries],expected=None if expected is None else expected._wire()),owner,"modeling.spatial"))["job"]\n',None))

use js_sys::Promise;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::future_to_promise;
use web_sys::*;

macro_rules! log {
    ( $( $t:tt )* ) => {
        web_sys::console::log_1(&format!( $( $t )* ).into());
    }
}

fn request_animation_frame(session: &XrSession, f: &Closure<dyn FnMut(f64, XrFrame)>) -> i32 {
    session.request_animation_frame(f.as_ref().unchecked_ref())
}

#[wasm_bindgen]
pub fn create_webgl_context(xr_mode: bool) -> Result<WebGl2RenderingContext, JsValue> {
    let canvas = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();

    let gl: WebGl2RenderingContext = if xr_mode {
        let mut gl_attribs = HashMap::new();
        gl_attribs.insert(String::from("xrCompatible"), true);
        let js_gl_attribs = JsValue::from_serde(&gl_attribs).unwrap();

        canvas
            .get_context_with_context_options("webgl2", &js_gl_attribs)?
            .unwrap()
            .dyn_into()?
    } else {
        canvas.get_context("webgl2")?.unwrap().dyn_into()?
    };

    Ok(gl)
}

#[wasm_bindgen]
pub struct XrApp {
    session: Rc<RefCell<Option<XrSession>>>,
    ref_space: Rc<RefCell<Option<XrReferenceSpace>>>,
    gl: Rc<WebGl2RenderingContext>,
}

#[wasm_bindgen]
impl XrApp {
    #[wasm_bindgen(constructor)]
    pub fn new() -> XrApp {
        console_error_panic_hook::set_once();
        let session = Rc::new(RefCell::new(None));
        let ref_space = Rc::new(RefCell::new(None));

        let xr_mode = true;
        let gl = Rc::new(create_webgl_context(xr_mode).unwrap());

        XrApp {
            session,
            ref_space,
            gl,
        }
    }

    pub fn init(&self) -> Promise {
        log!("Starting WebXR...");
        let navigator: web_sys::Navigator = web_sys::window().unwrap().navigator();
        let xr = navigator.xr();
        let session_mode = XrSessionMode::ImmersiveVr;
        let session_supported_promise = xr.is_session_supported(session_mode);

        let session = self.session.clone();
        let ref_space = self.ref_space.clone();
        let gl = self.gl.clone();

        let future = async move {
            let supports_session =
                wasm_bindgen_futures::JsFuture::from(session_supported_promise).await;
            let supports_session = supports_session.unwrap();
            if supports_session == false {
                log!("XR session not supported");
                return Ok(JsValue::from(false));
            }

            let mut xr_session_init = XrSessionInit::new();
            xr_session_init.optional_features(&JsValue::from_serde(&["bounded-floor"]).unwrap());
            let xr_session_promise =
                xr.request_session_with_options(session_mode, &xr_session_init);
            let xr_session = wasm_bindgen_futures::JsFuture::from(xr_session_promise).await;
            let xr_session: XrSession = xr_session.unwrap().into();

            let xr_gl_layer = XrWebGlLayer::new_with_web_gl2_rendering_context(&xr_session, &gl)?;
            let mut render_state_init = XrRenderStateInit::new();
            render_state_init.base_layer(Some(&xr_gl_layer));
            xr_session.update_render_state_with_state(&render_state_init);

            let ref_space_promise =
                xr_session.request_reference_space(XrReferenceSpaceType::BoundedFloor);
            let xr_ref_space = wasm_bindgen_futures::JsFuture::from(ref_space_promise).await;
            let xr_ref_space: XrReferenceSpace = xr_ref_space.unwrap().into();

            let mut session = session.borrow_mut();
            session.replace(xr_session);

            let mut ref_space = ref_space.borrow_mut();
            ref_space.replace(xr_ref_space);

            Ok(JsValue::from(true))
        };

        future_to_promise(future)
    }

    pub fn start(&self) {
        let session: &Option<XrSession> = &self.session.borrow();
        let sess: &XrSession = if let Some(sess) = session {
            sess
        } else {
            return ();
        };

        let f = Rc::new(RefCell::new(None));
        let g = f.clone();

        let gl = self.gl.clone();
        let ref_space = self.ref_space.clone();

        let shader_profram = gl.create_program().unwrap();

        let vs = gl
            .create_shader(WebGl2RenderingContext::VERTEX_SHADER)
            .unwrap();
        gl.shader_source(
            &vs,
            "#version 300 es
uniform mat4 model;
uniform mat4 view;
uniform mat4 projection;
in vec3 vertexPosition;
in vec3 vertexColor;
out vec3 vColor;
void main() {
    vColor = vertexColor;
    gl_Position = projection * view * model * vec4(vertexPosition, 1.0);
}",
        );
        gl.compile_shader(&vs);
        gl.attach_shader(&shader_profram, &vs);

        let fs = gl
            .create_shader(WebGl2RenderingContext::FRAGMENT_SHADER)
            .unwrap();
        gl.shader_source(
            &fs,
            "#version 300 es
precision highp float;
in vec3 vColor;
out vec4 fragmentColor;
void main() {
    fragmentColor = vec4(vColor,1);
}",
        );
        gl.compile_shader(&fs);
        gl.attach_shader(&shader_profram, &fs);

        gl.link_program(&shader_profram);

        if !gl
            .get_program_parameter(&shader_profram, WebGl2RenderingContext::LINK_STATUS)
            .as_bool()
            .unwrap_or(false)
        {
            log!(
                "program link errror:{}",
                gl.get_program_info_log(&shader_profram).unwrap()
            );

            if !gl
                .get_shader_parameter(&vs, WebGl2RenderingContext::COMPILE_STATUS)
                .as_bool()
                .unwrap_or(false)
            {
                log!("vs compile errror:{}", gl.get_shader_info_log(&vs).unwrap());
            }

            if !gl
                .get_shader_parameter(&fs, WebGl2RenderingContext::COMPILE_STATUS)
                .as_bool()
                .unwrap_or(false)
            {
                log!("fs compile errror:{}", gl.get_shader_info_log(&fs).unwrap());
            }
        }

        gl.enable(WebGl2RenderingContext::DEPTH_TEST);
        gl.enable(WebGl2RenderingContext::CULL_FACE);
        gl.use_program(Some(&shader_profram));

        let model_location = gl.get_uniform_location(&shader_profram, "model").unwrap();
        let view_location = gl.get_uniform_location(&shader_profram, "view").unwrap();
        let projection_location = gl
            .get_uniform_location(&shader_profram, "projection")
            .unwrap();

        let vertices: [f32; 18] = [
            -0.7, -0.7, 0.0, 1., 0., 0., 0.7, -0.7, 0.0, 0., 1., 0., 0.0, 0.7, 0.0, 0., 0., 1.,
        ];
        let vb = gl.create_buffer().unwrap();
        gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&vb));
        unsafe {
            let vertexies = js_sys::Float32Array::view(&vertices);
            gl.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ARRAY_BUFFER,
                &vertexies,
                WebGl2RenderingContext::STATIC_DRAW,
            );
        }

        let vertex_attrib_location = gl.get_attrib_location(&shader_profram, "vertexPosition");

        gl.enable_vertex_attrib_array(vertex_attrib_location as u32);
        gl.enable_vertex_attrib_array(1);

        gl.vertex_attrib_pointer_with_i32(
            0,
            3,
            WebGl2RenderingContext::FLOAT,
            false,
            (3 + 3) * 4,
            0,
        );
        gl.vertex_attrib_pointer_with_i32(
            1,
            3,
            WebGl2RenderingContext::FLOAT,
            false,
            (3 + 3) * 4,
            3 * 4,
        );

        *g.borrow_mut() = Some(Closure::wrap(Box::new(move |_time: f64, frame: XrFrame| {
            let sess: XrSession = frame.session();

            let gl_layer = sess.render_state().base_layer().unwrap();

            gl.bind_framebuffer(
                WebGl2RenderingContext::FRAMEBUFFER,
                Some(&gl_layer.framebuffer()),
            );
            gl.clear_color(0., 0., 0., 1.);
            gl.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);

            let ref_pose = ref_space.borrow();
            let pose = frame.get_viewer_pose(&ref_pose.as_ref().unwrap()).unwrap();
            let views = pose.views();
            gl.uniform_matrix4fv_with_f32_array(
                Some(&model_location),
                false,
                &[
                    2., 0., 0., 0., 0., 2., 0., 0., 0., 0., 2., 0., 0., 0., 0., 1.,
                ],
            );
            {
                let view: XrView = views.get(0).into();
                let vp = gl_layer.get_viewport(&view).unwrap();
                gl.viewport(vp.x(), vp.y(), vp.width(), vp.height());

                gl.uniform_matrix4fv_with_f32_array(
                    Some(&projection_location),
                    false,
                    &view.projection_matrix(),
                );
                gl.uniform_matrix4fv_with_f32_array(
                    Some(&view_location),
                    false,
                    &view.transform().inverse().matrix(),
                );
                gl.draw_arrays(WebGlRenderingContext::TRIANGLES, 0, (3) as i32);
            }
            {
                let view: XrView = views.get(1).into();
                let vp = gl_layer.get_viewport(&view).unwrap();
                gl.viewport(vp.x(), vp.y(), vp.width(), vp.height());

                gl.uniform_matrix4fv_with_f32_array(
                    Some(&projection_location),
                    false,
                    &view.projection_matrix(),
                );
                gl.uniform_matrix4fv_with_f32_array(
                    Some(&view_location),
                    false,
                    &view.transform().inverse().matrix(),
                );
                gl.draw_arrays(WebGlRenderingContext::TRIANGLES, 0, (3) as i32);
            }

            request_animation_frame(&sess, f.borrow().as_ref().unwrap());
        }) as Box<dyn FnMut(f64, XrFrame)>));

        request_animation_frame(sess, g.borrow().as_ref().unwrap());
    }
}

macro_rules! impl_webgl_trait {
    ($trait_name:ident implementors {$($implementor:ident;)*} methods {$(fn $method:ident($($arg_name:ident: $arg_type:ty),*) -> $result_type:ty;)*}) => {
        impl_webgl_trait!{trait $trait_name {$(fn $method($($arg_name: $arg_type),*) -> $result_type;)*}}
        impl_webgl_trait!{impl $trait_name implementors {$($implementor;)*} methods {$(fn $method($($arg_name: $arg_type),*) -> $result_type;)*}}
    };
    (trait $trait_name:ident {$(fn $method:ident($($arg_name:ident: $arg_type:ty),*) -> $result_type:ty;)*}) => {
        pub trait $trait_name {
            $(
                fn $method(&self, $($arg_name: $arg_type),*) -> $result_type;
            )*
        }
    };
    (impl $trait_name:ident implementors {$impl_head:ident; $($impl_tail:ident;)*} methods {$(fn $method:ident($($arg_name:ident: $arg_type:ty),*) -> $result_type:ty;)*}) => {
        impl $trait_name for $impl_head {
            $(
                fn $method(&self, $($arg_name: $arg_type),*) -> $result_type {
                    Self::$method(self, $($arg_name),*)
                }
            )*
        }
        impl_webgl_trait!{impl $trait_name implementors {$($impl_tail;)*} methods {$(fn $method($($arg_name: $arg_type),*) -> $result_type;)*}}
    };
    (impl $trait_name:ident implementors {} methods {$(fn $method:ident($($arg_name:ident: $arg_type:ty),*) -> $result_type:ty;)*}) => {};
}

impl_webgl_trait! {
    GlContext
    implementors {
        WebGlRenderingContext;
        WebGl2RenderingContext;
    }
    methods {
        fn attach_shader(program: &WebGlProgram, shader: &WebGlShader) -> ();
        fn bind_buffer(target: u32, buffer: Option<&WebGlBuffer>) -> ();
        fn bind_framebuffer(target: u32, framebuffer: Option<&WebGlFramebuffer>) -> ();
        fn blend_func(sfactor: u32, dfactor: u32) -> ();
        fn buffer_data_with_array_buffer_view(target: u32, src_data: &js_sys::Object, usage: u32) -> ();
        fn buffer_data_with_i32(target: u32, size: i32, usage: u32) -> ();
        fn clear(mask: u32) -> ();
        fn clear_color(red: f32, green: f32, blue: f32, alpha: f32) -> ();
        fn compile_shader(shader: &WebGlShader) -> ();
        fn create_buffer() -> Option<WebGlBuffer>;
        fn create_program() -> Option<WebGlProgram>;
        fn create_shader(type_: u32) -> Option<WebGlShader>;
        fn depth_func(func: u32) -> ();
        fn disable(cap: u32) -> ();
        fn draw_arrays(mode: u32, first: i32, count: i32) -> ();
        fn enable(cap: u32) -> ();
        fn enable_vertex_attrib_array(index: u32) -> ();
        fn get_active_attrib(program: &WebGlProgram, index: u32) -> Option<WebGlActiveInfo>;
        fn get_active_uniform(program: &WebGlProgram, index: u32) -> Option<WebGlActiveInfo>;
        fn get_attrib_location(program: &WebGlProgram, name: &str) -> i32;
        fn get_program_info_log(program: &WebGlProgram) -> Option<String>;
        fn get_program_parameter(program: &WebGlProgram, pname: u32) -> wasm_bindgen::JsValue;
        fn get_shader_info_log(shader: &WebGlShader) -> Option<String>;
        fn get_shader_parameter(shader: &WebGlShader, pname: u32) -> wasm_bindgen::JsValue;
        fn get_uniform_location(program: &WebGlProgram, name: &str) -> Option<WebGlUniformLocation>;
        fn link_program(program: &WebGlProgram) -> ();
        fn shader_source(shader: &WebGlShader, source: &str) -> ();
        fn uniform1f(location: Option<&WebGlUniformLocation>, x: f32) -> ();
        fn uniform2f(location: Option<&WebGlUniformLocation>, x: f32, y: f32) -> ();
        fn uniform_matrix4fv_with_f32_array(location: Option<&WebGlUniformLocation>, transpose: bool, data: &[f32]) -> ();
        fn use_program(program: Option<&WebGlProgram>) -> ();
        fn viewport(x: i32, y: i32, width: i32, height: i32) -> ();
        fn vertex_attrib_pointer_with_i32(index: u32, size: i32, type_: u32, normalized: bool, stride: i32, offset: i32) -> ();
    }
}

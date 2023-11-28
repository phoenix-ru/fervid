import HtmlWebpackPlugin from 'html-webpack-plugin'
import { webpackPlugin as VuePlugin } from '../../unplugin/index.mjs'

export default {
    mode: 'development',
    entry: {
        app: './index.js',
    },
    devServer: {
        static: './dist'
    },
    optimization: {
        runtimeChunk: 'single',
    },
    plugins: [
        new HtmlWebpackPlugin({
            title: 'Hello World App',
            template: './index.html'
        }),
        VuePlugin({ hmr: true, mode: 'development' })
    ]
}
